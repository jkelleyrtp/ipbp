use crate::introspect;
use dioxus::prelude::*;
use libloading::Library;

/// loop, check for changes to source, reload, and patch
pub fn use_background_hotreloader(fnc: fn() -> Element) -> Element {
    let library = use_signal(|| None as Option<Library>);
    println!("rendering");

    use_future(move || async move {
        let mut source = None;
        let mut patched_lib: Option<Library> = None as Option<Library>;
        // let mut fn_ptr = my_fn_ptr.as_mut() as *mut dyn Fn() -> Element;

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            introspect::patch_with_forward_symbol(library, &mut source, &mut patched_lib);
        }
    });

    library.with(|f| {
        if let Some(lib) = f {
            println!("Using patched library");
            let fn_ptr = unsafe { lib.get::<unsafe extern "C" fn() -> Element>(b"some_func_1") };
            let el = unsafe { fn_ptr.unwrap()() };
            println!("it's patched!");
            el
        } else {
            fnc()
        }
    })
}
