pub use dioxus::desktop::window;
use dioxus::prelude::*;
use libloading::Library;
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use std::{collections::HashMap, env, fs, path::PathBuf};
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
            unsafe {
                lib.get::<unsafe extern "C" fn() -> Element>(name.as_bytes())
                    .unwrap()()
            }
        } else {
            initial()
        }
    })
}
