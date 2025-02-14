use std::{
    env, fs,
    path::{Path, PathBuf},
};

use dioxus::{desktop::window, prelude::*};
use libloading::Library;
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use tokio::io::AsyncBufReadExt;

fn use_hotreload_component() -> Element {
    let mut library = use_signal(|| None as Option<Library>);

    use_hook(|| {
        spawn(async move {
            let stdin = tokio::io::stdin();
            let stdin = tokio::io::BufReader::new(stdin);
            let mut lines = stdin.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let so = PathBuf::from(line);

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
                        // println!("looking for symbol: {:?}", sym.name().unwrap());
                        if us_obj.symbol_by_name(sym.name().unwrap()).is_none() {
                            missing_symbols.push(sym.name().unwrap());
                            println!("Symbol not found on import: {:?}", sym.name().unwrap());
                        }
                    }
                }

                let lib = unsafe { libloading::Library::new(so).ok().unwrap() };
                let old = library.replace(Some(lib));

                if let Some(old) = old {
                    std::mem::forget(old);
                }
            }
        })
    });

    library.with(|f| {
        if let Some(lib) = f {
            let fn_ptr =
                unsafe { lib.get::<unsafe extern "C" fn() -> Element>(b"NewHotreloadComponent") };
            unsafe { fn_ptr.unwrap()() }
        } else {
            HotreloadComponent()
        }
    })
}

#[no_mangle]
#[inline(never)]
pub fn HotreloadComponent() -> Element {
    let s = "oh";

    let abc = 123;

    rsx! {
        div { "you're wrong reload masde? " }
    }
}

#[no_mangle]
#[inline(never)]
pub fn NewHotreloadComponent() -> Element {
    let s = "oh golly oh gosh";

    let abc = 128390128;

    // rsx! {
    //     div { "you're wrong rasdeload masde? " }
    //     div { "you're wrong rasdeload masde? " }
    //     div { "you're wrong rasdeload masde? " }
    // }

    // rsx! { "yu're just bad" }
    rsx! {
        div { "yu're just bad123123 {abc}" }
        div { "{s}" }
        button {
            onclick: move |_| {
                window().webview.zoom(2.0).unwrap();
            },
            "Zoom"
        }
        button {
            onclick: move |_| {
                window().webview.zoom(1.0);
            },
            "Reset zoom"
        }
        button {
            onclick: move |_| {
                window().webview.zoom(0.5);
            },
            "Zoom Out"
        }
    }
}
