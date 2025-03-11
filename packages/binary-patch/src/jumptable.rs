use std::collections::HashMap;

#[derive(serde::Deserialize, Debug)]
pub struct JumpTableRead {
    pub map: HashMap<u64, u64>,

    /// the address of the main function in the new original binary
    pub new_main_address: u64,

    /// the address of the main function in the old original binary
    pub old_main_address: u64,
}

// unsafe { libloading::os::unix::Library::new(Some(so), RTLD_NOW | RTLD_GLOBAL).unwrap() };
// unsafe { libloading::os::unix::Library::open(Some(so), RTLD_NOW | RTLD_GLOBAL).unwrap() };
// unsafe { dladdr(main_sym, &mut info) };
// exec header is 0x11C240000
// original calc is right...
// 0x100029b9c
// 0x100029b9c

// 0x21fe8fa9c - 0x125dedb9c + 0x125dc4000 = 0x21fe65f00 bur we're using base_address of 0x11fe65f00.
//
// 0x125dedb9c is the actual symbol
// burt we're using 0x21fe8fa9c

// use dladdr to get the baseaddress of the binary. This will be used to fix the jump table since the base address is usually 0
// let mut info = Dl_info {
//     dli_fname: null_mut(),
//     dli_fbase: null_mut(),
//     dli_sname: null_mut(),
//     dli_saddr: null_mut(),
// };

// macos api for getting offset
//
// // Method 2: Use dyld functions to iterate through loaded images
// uint32_t image_count = _dyld_image_count();
// for (uint32_t i = 0; i < image_count; i++) {
//     const char *name = _dyld_get_image_name(i);
//     if (strstr(name, "libname.dylib")) {
//         const struct mach_header *header = _dyld_get_image_header(i);
//         intptr_t slide = _dyld_get_image_vmaddr_slide(i);
//         void *base_address = (void *)header;
//         break;
//     }
// }

// we might find this C code useful:
//
// static int
// callback(struct dl_phdr_info *info, size_t size, void *data)
// {
//   int j;
//   const char *cb = (const char *)&callback;
//   const char *base = (const char *)info->dlpi_addr;
//   const ElfW(Phdr) *first_load = NULL;

//   for (j = 0; j < info->dlpi_phnum; j++) {
//     const ElfW(Phdr) *phdr = &info->dlpi_phdr[j];

//     if (phdr->p_type == PT_LOAD) {
//       const char *beg = base + phdr->p_vaddr;
//       const char *end = beg + phdr->p_memsz;

//       if (first_load == NULL) first_load = phdr;
//       if (beg <= cb && cb < end) {
//         // Found PT_LOAD that "covers" callback().
//         printf("ELF header is at %p, image linked at 0x%zx, relocation: 0x%zx\n",
//                base + first_load->p_vaddr, first_load->p_vaddr, info->dlpi_addr);
//         return 1;
//       }
//     }
//   }
//   return 0;
// }

// int
// main(int argc, char *argv[])
// {
//   dl_iterate_phdr(callback, NULL);
//   exit(EXIT_SUCCESS);
// }

// we should maybe modify the lookups to not be in the deps folder;
//
// https://stackoverflow.com/questions/9922949/how-to-print-the-ldlinker-search-path
//
// called `Result::unwrap()` on an `Err` value: DlOpen { desc: "dlopen(, 0x0009): tried: \'\' (no such file),
//  \'/System/Volumes/Preboot/Cryptexes/OS\' (not a file), \'/usr/lib/\' (not a file, not in dyld cache),
// \'\' (no such file),
// \'/Users/jonkelley/Development/Tinkering/ipbp/target/debug/deps/\' (not a file),
// \'/Users/jonkelley/Development/Tinkering/ipbp/target/debug/\' (not a file),
// \'/Users/jonkelley/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/lib/\' (not a file),
//  \'/Users/jonkelley/.rustup/toolchains/stable-aarch64-apple-darwin/lib/\' (not a file),
// \'/Users/jonkelley/lib/\' (no such file),
// \'/usr/local/lib/\' (not a file),
// \'/usr/lib/\' (not a file, not in dyld cache)" }

// /// Waits for stdin to send a new library
// pub fn use_hotreload_component(name: &str, initial: fn() -> Element) -> Element {
//     let mut library = use_signal(|| None as Option<&'static mut Library>);
//     let mut libraries = use_signal(|| vec![]);

//     use_hook(|| {
//         spawn(async move {
//             let stdin = tokio::io::stdin();
//             let stdin = tokio::io::BufReader::new(stdin);
//             let mut lines = stdin.lines();
//             while let Ok(Some(line)) = lines.next_line().await {
//                 let so = PathBuf::from(line);

//                 // we *need* to leak the library otherwise it will cause issues with the process not exiting properly
//                 use libloading::os::unix::{RTLD_GLOBAL, RTLD_LAZY};
//                 let lib = unsafe {
//                     libloading::os::unix::Library::open(Some(so), RTLD_LAZY | RTLD_GLOBAL).unwrap()
//                 };
//                 // let lib = unsafe { libloading::Library::new(so).unwrap() };
//                 let old = library.replace(Some(Box::leak(Box::new(lib))));

//                 // don't forget the old library - but require its drop to be called
//                 if let Some(old) = old {
//                     libraries.write().push(old);
//                 }
//             }
//         })
//     });

//     library.with(|f| {
//         if let Some(lib) = f {
//             unsafe {
//                 lib.get::<unsafe extern "C" fn() -> Element>(name.as_bytes())
//                     .unwrap()()
//             }
//         } else {
//             initial()
//         }
//     })
// }

// #[link_section = ".hot_fns"]
// fn make_thing() {}

// mod __META_make_thing {
//     #[link_section = ".meta.hot_fns"]
//     static make_thing_meta: &[u8] = module_path!().as_bytes();
// }

// patchfile {
//     code,
//     changed_roots,
//     changed_symbols,
//     statics?
// }

struct HotFn<T> {
    f: T,
    // ptr: [u8; 8],
    location: &'static mut &'static std::panic::Location<'static>,
}

#[track_caller]
const fn register<T: Copy>(t: T) -> HotFn<T> {
    static mut REGISTERED: &'static std::panic::Location<'static> = std::panic::Location::caller();

    // let p = unsafe { std::mem::transmute(t) };

    HotFn {
        f: t,
        // ptr: p,
        location: unsafe { &mut REGISTERED },
    }
}

#[test]
fn itworks() {
    fn some_loadable_function() -> String {
        "hello".to_string()
    }
    fn some_other_fn() -> i32 {
        123
    }
    fn some_other_fn2() -> String {
        "hello".to_string()
    }

    let p = register(if true {
        some_loadable_function as fn() -> String
    } else {
        some_other_fn2
    });
    let p2 = register(some_loadable_function);

    let r = type_name_of_val(&p.f);
    println!("r: {r}");

    let p = p.f as *const fn() -> String;
    println!("p: {p:?}");

    let p = some_loadable_function as *const fn() -> String;
    println!("p: {p:?}");

    let p = (p2.f) as *const fn() -> String;
    println!("p: {p:?}");

    // println!("p: {:?}", p2.ptr as *const fn() -> String);
}
