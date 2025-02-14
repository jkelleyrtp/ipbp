pub use dioxus::desktop::window;
use dioxus::prelude::*;
use libloading::Library;
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tokio::io::AsyncBufReadExt;

pub use hotreload_macro::hotreload_start as patched;

pub fn use_hotreload_component(name: &str, initial: fn() -> Element) -> Element {
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
            let fn_ptr = unsafe { lib.get::<unsafe extern "C" fn() -> Element>(name.as_bytes()) };
            unsafe { fn_ptr.unwrap()() }
        } else {
            initial()
        }
    })
}

// dioxus is a ui library written in rust to build web, desktop, and mobile applications
// users write their apps as if they were react but with a rusty dialect.
//
// instead of (T, set(T)) = use_state(), dioxus provides use_signal() -> Signal<T>
//
// Signal supports .set(T), .get(), .read(), .write(), and .with(f)
//
// When building desktop apps, dioxus provides a number of APIs for interacting with the operating system.
//
// For example, to set the zoom of the window, you can use the dioxus_desktop::window() API:
//
// ```
// // set zoom
// window().webview.zoom(2.0).unwrap();
//
// // reset zoom
// window().webview.zoom(1.0).unwrap();
//
// // zoom out
// window().webview.zoom(0.5).unwrap();
// ```
//
//
// To create elements with dioxus (like JSX in react), you can use the rsx! macro:
// ```
// rsx! {
//         div {
//             h1 { "Hotrleoad" }
//             button {
//                 onclick: move |_| {
//                     window().webview.zoom(2.0).unwrap();
//                 },
//                 "Zoom in"
//             }
//         }
//     }
// }
// ```
//
// Typically you would wrap your rsx in a component:
//
// ```
// fn app() -> Element {
//     rsx! {
//         div {
//             h1 { "Hotrleoad" }
//             button {
//                 onclick: move |_| {
//                     window().webview.zoom(2.0).unwrap();
//                 },
//                 "Zoom in"
//             }
//         }
//     }
// }
//
// fn main() {
//     dioxus::launch(app);
// }
// ```
//
// Note that onclick handlers are normal rust closures that take an Event<T> argument.
// Children in elements must follow the attributes.
//
// Note that we're using Dioxus 0.6 which does *NOT* take the `cx` argument.
//
// Components look like this:
//
// ```
// fn app() -> Element {
//     rsx! {}
// }
// ```
//
// The `cx` syntax is from Dioxus 0.4 and was removed in 0.5. We're using 0.6
//
// When signals are called using the `signal()` syntax, they return their current value.
//
// ```
// let mut zoom = use_signal(|| 1.0);
// let cur = zoom();
// ```
//
// For a component that stores its state in a signal, you can use the use_signal() hook combined with
// use_effect. Make sure to remember `move` annotations:
//
// fn app() -> Element {
//     let mut zoom = use_signal(|| 1.0);
//     use_effect(move || {
//         dioxus::desktop::window().webview.zoom(zoom());
//     });
//     rsx! {
//         div {
//             button {
//                 onclick: move |_| {
//                     zoom.set(zoom() * 2.0);
//                 },
//                 "Zoom 2.0"
//             }
//             button {
//                 onclick: move |_| {
//                     zoom.set(zoom() / 2.0);
//                 },
//                 "Zoom 0.5"
//             }
//         }
//     }
// }
//
// here's a sample:
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
