pub use dioxus::desktop::window;
use dioxus::prelude::*;
use libloading::Library;
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use std::{env, fs, path::PathBuf};
use tokio::io::AsyncBufReadExt;

pub use hotreload_macro::hotreload_start as start;

/// Waits for stdin to send a new library
pub fn use_hotreload_component(name: &str, initial: fn() -> Element) -> Element {
    let mut library = use_signal(|| None as Option<&'static mut Library>);
    let mut libraries = use_signal(|| vec![]);

    use_hook(|| {
        spawn(async move {
            let stdin = tokio::io::stdin();
            let stdin = tokio::io::BufReader::new(stdin);
            let mut lines = stdin.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let so = PathBuf::from(line);

                // we *need* to leak the library otherwise it will cause issues with the process not exiting properly
                let lib = unsafe { libloading::Library::new(so).ok().unwrap() };
                let old = library.replace(Some(Box::leak(Box::new(lib))));

                // don't forget the old library - but require its drop to be called
                if let Some(old) = old {
                    libraries.write().push(old);
                }
            }
        })
    });

    library.with(|f| {
        if let Some(lib) = f {
            let fn_ptr = unsafe { lib.get::<unsafe extern "C" fn() -> Element>(name.as_bytes()) };
            unsafe { fn_ptr.unwrap()() }
        } else {
            initial()
        }
    })
}

fn verify_library(so: &PathBuf) -> Vec<String> {
    // lets load the library and then try to resolve its missing symbols from our process
    let fread = fs::read(&so).unwrap();
    let obj = object::File::parse(&fread as &[u8]).unwrap();

    // Open ourself as a file too
    let file = std::fs::File::open(env::current_exe().unwrap()).unwrap();
    let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };
    let us_obj = object::File::parse(&*mmap).unwrap();

    let mut missing_symbols = vec![];
    for sym in obj.symbols() {
        if sym.is_undefined() {
            if us_obj.symbol_by_name(sym.name().unwrap()).is_none() {
                missing_symbols.push(sym.name().unwrap().to_string());
                println!("Symbol not found on import: {:?}", sym.name().unwrap());
            }
        }
    }

    missing_symbols
}
