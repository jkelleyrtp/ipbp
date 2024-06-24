mod dioxus_support;
mod introspect;
mod object_from_patch;
mod strip_tls;

use dioxus::prelude::*;

fn main() {
    launch(|| dioxus_support::use_background_hotreloader(some_func_1));
}

#[no_mangle]
#[inline(never)]
pub fn some_func_1() -> Element {
    let value = "This is real binary patching 🐱";
    let mut count = use_signal(|| 0);
    let mut names = use_signal(|| vec!["John".to_string(), "Doe".to_string()]);

    dioxus::desktop::window().webview.zoom(2.0);
    // dioxus::desktop::window().webview.zoom(1.0);

    println!("rendering1a12s23asdkassssdaas3sad");

    //
    rsx! {
        // Counter { id: 865 }
        li { background_color: "red", "That's not good :/ valusde: {value}" }
        li { background_color: "green", "This is value: {value}" }
        li { "This123usde123123: {value}" }
        li { "This issd vaasdsdasdadsasdlue: 123{value}" }
        li { "This is valsdue: {value}" }
        ul {
            for item in 0..2 {
                div {
                    li { "Some123 thats crazy w123123orks?!121233  {item}" }
                    li { "Some123 thats crazy w123123orks?!121233  {item}" }
                    li { "Some123 thats crazy w123123orks?!121233  {item}" }
                }
            }
            br {}
            button {
                onclick: move |_| {
                    count += 1;
                    names.push("Newashdkjassh 3 Name".to_string());
                    names.push("Newashdkjash 1 Name".to_string());
                },
                "Icrement"
            }
            for name in names.iter() {
                li { "{name}" }
            }
            button {
                onclick: move |_| {
                    names.push("Newashdkjash 123 Name".to_string());
                    names.clear();
                    count -= 1;
                },
                "Decremenet"
            }
            "Count {count}"
        }
    }
}

// #[no_mangle]
// #[inline(never)]
// #[component]
// fn Counter(id: i32) -> Element {
//     rsx! {
//         div { "This counter is cool {id}" }
//     }
// }
