mod dioxus_support;
mod introspect;
mod strip_tls;

use dioxus::prelude::*;

fn main() {
    launch(|| dioxus_support::use_background_hotreloader(some_func_1));
}

#[no_mangle]
#[inline(never)]
pub fn some_func_1() -> Element {
    let value = "This is true hotreloading 🐱";
    let mut count = use_signal(|| 0);

    rsx! {
        "This is value: {value}"
        ul {
            for item in 0..3 {
                li { "stop fighting me! {item}" }
            }
            br {}
            for item in 5..7 {
                li { "Sub item {item}" }
            }
            button {
                onclick: move |_| {
                    count += 1;
                },
                "Icrement"
            }
            button {
                onclick: move |_| {
                    count -= 1;
                },
                "Decremenet"
            }
            "Count {count}"
        }
    }
}
