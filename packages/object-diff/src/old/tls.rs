//! Support for TLS.
//!
//! In the doc and comment of this file, we use `module` and `plugin` interchangably.
use std::cell::RefCell;
use std::ops::Deref;

use memmap::MmapMut;
// use mmap::{Mmap, MmapOptions};a
use object::SectionKind;

use super::section::Section;
// use super::{Error, LOADED_MODULESa};

/// Phoenix plugins' mod_id will starting from this number.
pub(crate) const PHOENIX_MOD_BASE: usize = 1 << 30;
/// Invalid mod id.
pub(crate) const PHOENIX_MOD_INVALID: usize = 0;
/// A number identifying that the TLV is defined in the init binary,
/// and `phoenix_tls_get_addr` should use `__tls_get_addr` to resolve it.
/// `mod_id` = 1 is special for __tls_get_addr. It refers to the current binary.
pub(crate) const PHOENIX_MOD_INIT_EXEC: usize = 1;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PhoenixModId(pub(crate) usize);

impl PhoenixModId {
    /// The module is defined in the init binary.
    #[inline]
    pub(crate) const fn is_init_exec(&self) -> bool {
        self.0 == PHOENIX_MOD_INIT_EXEC
    }

    /// The module is a dynamic binary, should ask __tls_get_addr to resolve for us.
    #[inline]
    pub(crate) const fn is_dynamic_library(&self) -> bool {
        !self.is_init_exec() && self.0 < PHOENIX_MOD_BASE
    }

    /// The module is a phoenix plugin.
    #[inline]
    pub(crate) const fn is_phoenix_plugin(&self) -> bool {
        self.0 >= PHOENIX_MOD_BASE
    }

    #[inline]
    pub(crate) const fn is_valid(&self) -> bool {
        self.0 != PHOENIX_MOD_INVALID
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TlsIndex {
    pub(crate) mod_id: PhoenixModId,
    pub(crate) offset: usize,
}

impl TlsIndex {
    #[inline]
    pub(crate) fn new(mod_id: usize, offset: usize) -> Self {
        assert_ne!(mod_id, PHOENIX_MOD_INVALID);
        TlsIndex {
            mod_id: PhoenixModId(mod_id),
            offset,
        }
    }
}

#[derive(Debug)]
pub(crate) struct TlsInitImage {
    // sizeof .tbss + filesz
    _memsz: usize,
    // sizeof .tdata
    _filesz: usize,
    // the memory for the tls init image. None if memsz is zero.
    mmap: Option<MmapMut>,
}

impl TlsInitImage {
    // Create the TLS initialization image and modify the related sections to
    // point to offsets in this TLS init image.
    pub(crate) fn new(sections: &mut Vec<Section>) -> Result<Self, Error> {
        let mut memsz = 0;
        let mut filesz = 0;
        let mut maxalign = 0;

        // Do the first pass to compute memsz, filesz, and maxalign
        for sec in sections.iter() {
            let size = sec.size.next_multiple_of(sec.align.max(8)) as usize;
            match sec.kind {
                SectionKind::Tls => {
                    filesz += size;
                    memsz += size;
                    maxalign = maxalign.max(sec.align);
                }
                SectionKind::UninitializedTls => {
                    memsz += size;
                    maxalign = maxalign.max(sec.align);
                }
                _ => {}
            }
        }

        // no tls variables
        if memsz == 0 {
            return Ok(TlsInitImage {
                _memsz: 0,
                _filesz: 0,
                mmap: None,
            });
        }

        let size = memsz.next_multiple_of(page_size::get().max(maxalign as usize));
        let mut mmap = MmapOptions::new()
            .len(size)
            .anon(true)
            .private(true)
            .read(true)
            .write(true)
            .mmap()?;
        assert!(
            mmap.as_ptr().addr() as u64 % maxalign == 0,
            "align: {}",
            maxalign
        );

        // Do the second pass to do memcopy over filesz
        let mut tdata_off = memsz - filesz;
        let mut tbss_off = 0;
        for sec in sections {
            let size = sec.size.next_multiple_of(sec.align.max(8)) as usize;
            match sec.kind {
                SectionKind::Tls => {
                    mmap[tdata_off..tdata_off + size].copy_from_slice(unsafe {
                        std::slice::from_raw_parts(sec.address as *const u8, sec.size as usize)
                    });
                    sec.address = tdata_off as u64;
                    tdata_off += size;
                }
                SectionKind::UninitializedTls => {
                    sec.address = tbss_off as u64;
                    tbss_off += size;
                }
                _ => {}
            }
        }

        mmap[..tbss_off].fill(0);

        Ok(Self {
            _memsz: memsz,
            _filesz: filesz,
            mmap: Some(mmap),
        })
    }

    #[inline]
    pub(crate) fn as_slice(&self) -> &[u8] {
        self.mmap.as_ref().unwrap().deref()
    }
}

/// A read/write memory region for .tbss and .tdata with proper alignment in a module.
#[derive(Debug)]
struct TlsBlockInner {
    data: Box<[u8]>,
}

impl TlsBlockInner {
    #[inline]
    fn as_mut_ptr(&mut self) -> *mut () {
        self.data.as_mut_ptr().cast()
    }
}

type TlsBlock = Option<TlsBlockInner>;

/// Dynmic thread vector.
#[derive(Debug, Default)]
struct Dtv(Vec<TlsBlock>);

impl Dtv {
    #[inline]
    fn get_or_create(&mut self, ti: TlsIndex) -> &mut TlsBlock {
        let mod_id = ti.mod_id.0 - PHOENIX_MOD_BASE;
        if mod_id >= self.0.len() {
            println!("trigger a resize, in Dtv::get_or_create, ti: {:?}", ti);
            self.0.resize_with(mod_id + 1, || None);
        }

        if self.0[mod_id].is_none() {
            // Locate the matching mod_id from all loaded modules
            // Allocate and copy the TLS initialization image of module to the TLS block.
            let data = LOADED_MODULES
                .clone_tls_initimage(ti.mod_id.0)
                .unwrap_or_else(|| panic!("No such mod_id: {} found", ti.mod_id.0));
            println!(
                "trigger a tls_initimage clone, in Dtv::get_or_create, ti: {:?}",
                ti
            );

            self.0[mod_id] = Some(TlsBlockInner { data });
        }

        &mut self.0[mod_id]
    }
}

thread_local! {
    static DTV: RefCell<Dtv> = RefCell::new(Dtv::default());
}

// /// Returns a mutable reference to the thread's DTV.
// ///
// /// # Safety
// ///
// /// It is unsafe out of several reasons. First, the implementation assumes
// /// that the tid returned by the rust std library is continuous and compact.
// ///
// /// Second...
// unsafe fn get_dtv() -> &'static mut Dtv {
// A unique identifier for a running thread maintained by the rust
// standard library. The current implemention returns a counter
// starting from one, so it is continuous and compact and we could
// use it as an index into an array.
//
// The implemention of this has been stablized, so it is unlikely
// to be changed and the asummption should hold.
// let tid = std::thread::current().id().as_u64().get();

// static ALL_DTV: RefCell<Vec<Dtv>> = RefCell::new(Vec::new());
// if tid as usize >= ALL_DTV.len() {
//     ALL_DTV.resize_with(tid as usize + 1, || Dtv::default());
// }

// &mut ALL_DTV[tid as usize]
// }

/// A replacement implemention for libc's `__tls_get_addr` for plugins.
/// This function will be called from multiple threads concurrently.
/// __tls_get_addr does not use the default x86 calling convention of
/// passing arguments on the stack. Therefore, phoenix_tls_get_addr has to use the
/// same calling convention as __tls_get_addr.
#[no_mangle]
pub(crate) extern "C" fn phoenix_tls_get_addr(/*tls_index: &TlsIndex*/) -> *mut () {
    use std::arch::asm;
    let tls_index_addr: usize;
    unsafe {
        asm!("mov {}, rdi", out(reg) tls_index_addr);
    }
    // let start = minstant::Instant::now();
    DTV.with_borrow_mut(|dtv| {
        let tls_index: &TlsIndex = unsafe { &*(tls_index_addr as *const TlsIndex) };
        assert!(
            tls_index.mod_id.is_valid(),
            "Invalid mod_id: {}",
            PHOENIX_MOD_INVALID
        );
        // println!("tls_index: {:?}", tls_index);

        if tls_index.mod_id.is_phoenix_plugin() {
            let tls_block = dtv.get_or_create(*tls_index);

            // SAFETY: should be fine to unwrap because dtv.get_or_create should just created the block
            let ret = tls_block
                .as_mut()
                .unwrap()
                .as_mut_ptr()
                .map_addr(|addr| addr + tls_index.offset);
            // println!("ret: {:0x}, value: {:0x}", ret as usize, unsafe { ret.cast::<u64>().read() });
            // let dura = start.elapsed();
            // println!("phoenix_plugin_tls: {:?}", dura);
            ret
        } else if tls_index.mod_id.is_dynamic_library() || tls_index.mod_id.is_init_exec() {
            let ret = unsafe {
                asm!("mov rdi, {}", in(reg) tls_index_addr);
                __tls_get_addr(tls_index as *const TlsIndex as *mut TlsIndex as *mut libc::size_t)
            };
            // println!("ret: {:0x}", ret as usize);
            // let dura = start.elapsed();
            // println!("__tls_get_addr: {:?}", dura);
            ret as _
        } else {
            panic!("invalid tls_index: {:?}", tls_index);
        }
    })
}

extern "C" {
    pub fn __tls_get_addr(v: *mut libc::size_t) -> *mut libc::c_void;
}
