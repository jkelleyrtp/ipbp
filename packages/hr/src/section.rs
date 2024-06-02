use std::mem;
use std::ptr;

use memmap::{Mmap, MmapMut, MmapOptions};
use object::elf;
use object::elf::FileHeader64;
use object::endian::LittleEndian;
use object::read::elf::ElfSection;
use object::read::SymbolIndex;
use object::{ObjectSection, Relocation, SectionFlags, SectionIndex, SectionKind};
use page_size as pagesize;

use super::symbol::{ExtraSymbol, Symbol, SymbolTable};
use super::tls::TlsIndex;
// use super::Error;

#[derive(Debug)]
pub(crate) struct Section {
    #[allow(unused)]
    pub(crate) index: SectionIndex,
    pub(crate) address: u64,
    pub(crate) size: u64,
    pub(crate) align: u64,
    pub(crate) file_range: Option<(u64, u64)>,
    pub(crate) name: String,
    #[allow(unused)]
    pub(crate) segment_name: Option<String>,
    pub(crate) kind: SectionKind,
    #[allow(unused)]
    pub(crate) flags: SectionFlags,
    pub(crate) relocations: Vec<(u64, Relocation)>,
    /// For .bss/.tbss sections, we need to allocate extra spaces.
    pub(crate) mmap: Option<memmap::MmapMut>,

    // Not a mandatory field, used by tbss and tdata section to allocate slot for TLVs
    pub(crate) used: u64,
}

impl Section {
    pub(crate) fn new(section: &ElfSection<FileHeader64<LittleEndian>>) -> Self {
        Section {
            index: section.index(),
            // We will update the address later.
            address: section.address(),
            size: section.size(),
            align: section.align(),
            file_range: section.file_range(),
            name: section.name().unwrap_or("").to_owned(),
            segment_name: section.segment_name().unwrap_or(None).map(|x| x.to_owned()),
            kind: section.kind(),
            flags: section.flags(),
            relocations: section.relocations().collect::<Vec<(u64, Relocation)>>(),
            // For .bss/.tbss sections, we need to allocate extra spaces. We'll fill this later.
            mmap: None,
            used: 0,
        }
    }

    #[inline]
    pub(crate) fn need_load(&self) -> bool {
        if self.size == 0 {
            return false;
        }

        match self.kind {
            SectionKind::Text
            | SectionKind::Data
            | SectionKind::ReadOnlyData
            | SectionKind::Elf(elf::SHT_INIT_ARRAY)
            | SectionKind::Elf(elf::SHT_FINI_ARRAY)
            | SectionKind::Tls => true,
            _ => false,
        }
    }

    /// Update runtime address for sections needed to load. Allocate memory for .bss sections
    /// if encountered.
    pub(crate) fn update_runtime_addr(&mut self, image_start: *const u8) -> anyhow::Result<()> {
        if self.kind.is_bss() && self.size > 0 {
            // Allocate memory for .bss/.tbss section.
            // Strictly, there's no need to allocate for .tbss here.
            assert!(self.align as usize <= page_size::get());
            // round up to page
            let rounded_size = self
                .size
                .next_multiple_of(self.align)
                .next_multiple_of(page_size::get() as u64) as usize;
            let mmap = MmapOptions::new()
                .len(rounded_size)
                // .anon(true)
                // .private(true)
                // .read(true)
                // .write(true)
                // .mmap()?;
                .map_anon()?;
            // update the address
            self.address = mmap.as_ptr() as *const () as u64;
            self.mmap = Some(mmap);
        } else if self.need_load() {
            let file_off = self.file_range.expect("impossible").0;
            self.address = unsafe { image_start.offset(file_off as isize) } as *const () as u64;
            // if let Some((file_off, ..)) = self.file_range a
            //    self.address = unsafe { image_start.offset(file_off as isize) }.addr() as u64;
            // }
            // eprintln!(
            //     "section: {}, {:0x}, image_addr: {:0x}, file_off: {:0x}",
            //     self.name,
            //     self.address,
            //     image_start.addr(),
            //     file_off
            // );
            // eprintln!("code: {:?}", unsafe {
            //     std::slice::from_raw_parts(self.address as *const u8, 32)
            // });
        }
        Ok(())
    }

    // Allocate an offset for thread local variable
    pub(crate) fn alloc_tlv(&mut self, sym: &Symbol) -> u64 {
        assert!(self.used + sym.size <= self.size);
        assert_eq!(sym.address, 0);
        // here the `self.address` is is set to the offset into the tls_initimage.
        let ret = self.address + self.used;
        self.used += sym.size.next_multiple_of(8);
        // println!(
        //     "self.address: {}, ret: {}, sym.size: {}, sym.size_rounded: {}, self.used: {}",
        //     self.address,
        //     ret,
        //     sym.size,
        //     sym.size.next_multiple_of(8),
        //     self.used
        // );
        ret
    }
}

// All below are special sections (sections other than .rodata, .data, .text. .bss)

// Common symbols are a feature that allow a programmer to 'define' several
// variables of the same name in different source files.
// This is indeed 'common' in ELF relocatable object files.
pub(crate) struct CommonSection {
    mmap: Option<MmapMut>,
    used: isize,
}

impl CommonSection {
    pub(crate) fn new(sym_table: &SymbolTable) -> anyhow::Result<Self> {
        let size: u64 = sym_table
            .iter()
            .filter_map(|(_, sym)| if sym.is_common { Some(sym.size) } else { None })
            .sum();
        let size = (size as usize).next_multiple_of(page_size::get());
        let mmap = if size > 0 {
            Some(
                MmapOptions::new()
                    .len(size)
                    // .anon(true)
                    // .private(true)
                    // .read(true)
                    // .write(true)
                    // .mmap()?,
                    .map_anon()?,
            )
        } else {
            None
        };
        Ok(Self { mmap, used: 0 })
    }

    pub(crate) fn alloc_entry_for_symbol(&mut self, sym: &Symbol) -> *const u8 {
        let mmap = self
            .mmap
            .as_ref()
            .expect("Something is wrong with calculating common size");
        assert!(((self.used + sym.size as isize) as usize) < mmap.len());
        let ret = unsafe { mmap.as_ptr().offset(self.used) };
        self.used += sym.size as isize;
        ret
    }
}

// A combination of GOT and PLT. The layout is not exactly the same as the tradition
// but it is simpler to implement.
pub(crate) struct ExtraSymbolSection {
    mmap: Option<MmapMut>,
}

impl ExtraSymbolSection {
    pub(crate) fn new(num_symbols: usize) -> anyhow::Result<Self> {
        let size = num_symbols * mem::size_of::<ExtraSymbol>();
        let size = size.next_multiple_of(page_size::get());
        let mmap = if size > 0 {
            let mut mmap = MmapOptions::new().len(size).map_anon()?;
            // .anon(true)
            // .private(true)
            // .read(true)
            // .write(true)
            // .exec(true) /* we have trampoline code in this section */
            // .mmap()?;
            // zero-fill
            mmap.fill(0);
            Some(mmap)
        } else {
            None
        };
        Ok(Self { mmap })
    }

    #[inline]
    pub(crate) fn get_base_address(&self) -> usize {
        self.section_start().addr()
    }

    #[inline]
    pub(crate) fn section_start(&self) -> *mut ExtraSymbol {
        self.mmap
            .as_ref()
            .unwrap()
            .as_ptr()
            .cast::<ExtraSymbol>()
            .cast_mut()
    }

    /// Allocates an GOT entry in the section and returns the address of the entry.
    #[inline]
    pub(crate) fn make_got_entry(&self, sym_addr: usize, sym_index: SymbolIndex) -> usize {
        let start = self.section_start();
        let entry = unsafe { &mut *start.offset(sym_index.0 as isize) };
        *entry = ExtraSymbol {
            addr: sym_addr,
            /* ff 25 f2 ff ff ff    	jmp    *-0xe(%rip)  # where 0xe = 8 + 6 */
            trampoline: [0xFF, 0x25, 0xF2, 0xFF, 0xFF, 0xFF, 0x00, 0x00],
        };
        ptr::addr_of!(entry.addr).addr()
    }

    /// Allocates an GOT entry in the section and returns the address of the trampoline code.
    #[inline]
    pub(crate) fn make_plt_entry(&self, sym_addr: usize, sym_index: SymbolIndex) -> usize {
        let start = self.section_start();
        let entry = unsafe { &mut *start.offset(sym_index.0 as isize) };
        *entry = ExtraSymbol {
            addr: sym_addr,
            trampoline: [0xFF, 0x25, 0xF2, 0xFF, 0xFF, 0xFF, 0x00, 0x00],
        };
        ptr::addr_of!(entry.trampoline).addr()
    }

    /// Allocates an GOT entry in the section and returns the address of the entry.
    /// Rather than `addr` and `trampoline`, the content becomes `mod_id` and `offset`,
    /// which happen to be also 16 bytes.
    #[inline]
    pub(crate) fn make_got_entry_for_tls_index(
        &self,
        ti: TlsIndex,
        sym_index: SymbolIndex,
    ) -> usize {
        let start = self.section_start();
        debug_assert!(
            sym_index.0 as usize * mem::size_of::<ExtraSymbol>()
                < self.mmap.as_ref().unwrap().len()
        );
        let entry = unsafe { &mut *start.offset(sym_index.0 as isize) };
        *entry = ExtraSymbol {
            addr: ti.mod_id.0,
            trampoline: ti.offset.to_le_bytes(),
        };
        // *entry = unsafe { mem::transmute::<TlsIndex, ExtraSymbol>(ti) };
        ptr::addr_of!(entry.addr).addr()
    }
}
