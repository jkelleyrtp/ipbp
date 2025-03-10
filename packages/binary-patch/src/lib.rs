pub use dioxus::desktop::window;
use dioxus::{html::link::r#as, prelude::*};
use jumptable::JumpTableRead;
use libc::{dladdr, Dl_info};
use libloading::os::unix::{Library, RTLD_NOW};
use libloading::os::unix::{RTLD_GLOBAL, RTLD_LAZY};
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use std::{
    any::type_name_of_val, collections::HashMap, env, ffi::c_void, fs, mem::transmute_copy,
    ops::Deref, path::PathBuf, ptr::null_mut, sync::Arc,
};
use tokio::io::AsyncBufReadExt;

mod deref_helper;
pub mod jumptable;
pub mod subsecond;

pub use hotreload_macro::hotreload_start as start;

static mut APP_JUMP_TABLE: Option<JumpTableRead> = None;
static mut ORIGINAL_APP_MAIN: Option<fn() -> Element> = None;
static mut APP_MAIN: Option<fn() -> Element> = None;
static mut HOTRELOAD_HANDLERS: Vec<Arc<dyn Fn()>> = vec![];

pub fn hotreloadable(f: fn() -> Element) -> fn() -> Element {
    unsafe {
        let ptr = f as *const fn() -> Element;

        ORIGINAL_APP_MAIN = Some(f);
        APP_MAIN = Some(f);
    }

    inner_reloadable
}

pub fn inner_reloadable() -> Element {
    use_hook(|| unsafe {
        let needs_update = dioxus::prelude::schedule_update();
        HOTRELOAD_HANDLERS.push(needs_update);
    });

    println!("Rendering inner!");

    unsafe {
        if let Some(app_main) = APP_MAIN {
            app_main()
        } else {
            todo!()
        }
    }
}

#[no_mangle]
pub extern "C" fn hotfn_load_binary_patch(path: *const i8, jump_table_path: *const i8) {
    println!("executing hotfn_load_binary_patch... {path:?} {jump_table_path:?}");

    // Load the jump table by deserializing it from the file
    let jump_table_path_str =
        unsafe { std::ffi::CStr::from_ptr(jump_table_path).to_str().unwrap() };
    let jump_table_path = PathBuf::from(jump_table_path_str);
    let mut jump_table: JumpTableRead =
        bincode::deserialize(&std::fs::read(jump_table_path).unwrap()).unwrap();
    let path_str = unsafe { std::ffi::CStr::from_ptr(path).to_str().unwrap() };
    let so = PathBuf::from(path_str);
    let lib = unsafe { libloading::os::unix::Library::new(so).unwrap() };
    // unsafe { libloading::os::unix::Library::new(Some(so), RTLD_NOW | RTLD_GLOBAL).unwrap() };
    // unsafe { libloading::os::unix::Library::open(Some(so), RTLD_NOW | RTLD_GLOBAL).unwrap() };
    let lib = Box::leak(Box::new(lib));

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
    let exec_header = unsafe { lib.get::<*const ()>(b"_mh_execute_header") }
        .unwrap()
        .as_raw_ptr();
    // unsafe { dladdr(main_sym, &mut info) };

    // exec header is 0x11C240000
    let dl_offset = exec_header.wrapping_sub(0x0000000100000000);
    println!("Offset of binary in memory: {:?}", dl_offset as *const ());
    // for (old, new) in jump_table.map.iter_mut() {
    //     *new += dl_offset as u64;
    // }

    unsafe {
        let original = ORIGINAL_APP_MAIN.unwrap();
        let new_main = jump_table.map.get(&(original as u64)).unwrap().clone() as *mut c_void;
        // let sym: libloading::os::unix::Symbol<fn() -> Result<VNode, RenderError>> =
        //     unsafe { lib.get::<fn() -> Element>(b"__app") }.unwrap();

        // let f = sym.as_raw_ptr();
        // let actualfn = sym.deref().clone();

        // APP_MAIN = Some(actualfn);

        // let sym_raw_ptr = sym.as_raw_ptr();
        let guess = new_main.wrapping_add(dl_offset as usize);
        // println!("sym addr: {:?}", sym_raw_ptr); // 0x11c269b9c
        println!("new_main addr: {:?}", new_main); // 0x100029b9c
        println!("We guess it's at: {:?}", guess);

        // println!("actualfn: {:?}", actualfn);
        let _f = transmute_copy(&guess);
        println!("guess_f: {:?}", _f);
        APP_MAIN = Some(_f);

        // let new_main = (new_main as u64 + dl_offset) as *const fn() -> Element;
        // assert_eq!(new_main, sym_raw_ptr as *const fn() -> Element);

        // println!(
        //     "Patching main to: {:?} using _mh_execute_header: {:?} with base_address: {:?}. original: {:?}",
        //     new_main, exec_header, dl_offset as *const (), (new_main as u64 - dl_offset) as *const ()
        // );
        // APP_MAIN = Some(&*sym.into_raw());
        // APP_MAIN = Some(*&*new_main);
    }

    for handler in unsafe { HOTRELOAD_HANDLERS.iter() } {
        handler();
    }

    println!("Finished hotreload handler");
}

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
