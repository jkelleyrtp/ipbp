use crate::introspect;
use dioxus::prelude::*;

/// loop, check for changes to source, reload, and patch
pub fn use_background_hotreloader(fnc: fn() -> Element) -> Element {
    let mut fn_ptr = use_signal(|| fnc as fn() -> Element);
    println!("rendering");

    use_future(move || async move {
        let mut source = None;

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let mut my_fn_ptr = fnc as fn() -> Element;
            introspect::patch_with_forward_symbol(&mut my_fn_ptr, &mut source);

            // if the ptrs changed then update the fn_ptr
            if my_fn_ptr as *const () != fnc as *const () {
                println!("setting symbol");
                fn_ptr.set(my_fn_ptr);
            }
        }
    });

    fn_ptr.cloned()()
}
