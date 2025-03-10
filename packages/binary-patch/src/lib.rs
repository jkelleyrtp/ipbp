pub use dioxus::desktop::window;
use dioxus::prelude::*;
use jumptable::JumpTableRead;
use libc::{dladdr, Dl_info};
use libloading::os::unix::{Library, RTLD_NOW};
use libloading::os::unix::{RTLD_GLOBAL, RTLD_LAZY};
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use std::{
    any::type_name_of_val, collections::HashMap, env, ffi::c_void, fs, path::PathBuf,
    ptr::null_mut, sync::Arc,
};
use tokio::io::AsyncBufReadExt;

mod deref_helper;
pub mod jumptable;
pub mod subsecond;

pub use hotreload_macro::hotreload_start as start;

static mut APP_JUMP_TABLE: Option<JumpTableRead> = None;
static mut APP_SYM_NAME: Option<&'static str> = None;
static mut ORIGINAL_APP_MAIN: Option<*const fn() -> Element> = None;
static mut APP_MAIN: Option<*const fn() -> Element> = None;
static mut HOTRELOAD_HANDLERS: Vec<Arc<dyn Fn()>> = vec![];

pub fn hotreloadable(f: fn() -> Element) -> fn() -> Element {
    unsafe {
        let ptr = f as *const fn() -> Element;

        ORIGINAL_APP_MAIN = Some(f as *const fn() -> Element);
        APP_MAIN = Some(f as *const fn() -> Element);
        APP_SYM_NAME = {
            let mut info = Dl_info {
                dli_fname: null_mut(),
                dli_fbase: null_mut(),
                dli_sname: null_mut(),
                dli_saddr: null_mut(),
            };
            dladdr(ptr as *const c_void, &mut info);
            Some(std::ffi::CStr::from_ptr(info.dli_sname).to_str().unwrap())
        };

        println!("hotreloadable: {:?}", APP_SYM_NAME);
    }

    inner_reloadable
}

pub fn inner_reloadable() -> Element {
    use_hook(|| unsafe {
        let needs_update = dioxus::prelude::schedule_update();
        HOTRELOAD_HANDLERS.push(needs_update);
    });

    unsafe {
        if let Some(app_main) = APP_MAIN {
            let app_main = std::mem::transmute::<*const fn() -> Element, fn() -> Element>(app_main);
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
    let lib =
        unsafe { libloading::os::unix::Library::open(Some(so), RTLD_NOW | RTLD_GLOBAL).unwrap() };
    let lib = Box::leak(Box::new(lib));

    // use dladdr to get the baseaddress of the binary. This will be used to fix the jump table since the base address is usually 0
    let mut info = Dl_info {
        dli_fname: null_mut(),
        dli_fbase: null_mut(),
        dli_sname: null_mut(),
        dli_saddr: null_mut(),
    };
    let main_sym = unsafe { lib.get::<unsafe extern "C" fn() -> Element>(b"_main") }
        .unwrap()
        .as_raw_ptr();
    unsafe { dladdr(main_sym, &mut info) };
    let base_address = info.dli_fbase as u64;
    for (old, new) in jump_table.map.iter_mut() {
        *new += base_address;
    }

    unsafe {
        let original = ORIGINAL_APP_MAIN.unwrap();
        APP_MAIN =
            Some(jump_table.map.get(&(original as u64)).unwrap().clone() as *const fn() -> Element);
    }

    for handler in unsafe { HOTRELOAD_HANDLERS.iter() } {
        handler();
    }
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
