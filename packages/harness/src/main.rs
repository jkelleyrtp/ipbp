use std::any::TypeId;

use binary_patch::*;
use dioxus::prelude::*;

fn main() {
    dioxus::launch(app);
}

#[binary_patch::patched]
fn app() -> Element {
    rsx! {
        zoom_controls {}
    }
}

fn zoom_controls() -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        div {
            h1 { "Rust VIBE CODING it!!" }
            em { "powered by 123" }
            h2 { "Dioxus Binary Patching 💪" }
            button {
                onclick: move |_| {
                    count.set(count() + 1);
                },
                "Click me?"
            }
            button {
                onclick: move |_| {
                    count.set(count() + 2);
                },
                "Click me again?!!!"
            }
            div { "You wow that's insane it works {count() * 2} times" }
            button {
                onclick: move |_| {
                    window().webview.zoom(1.5).unwrap();
                },
                "Zoom in"
            }
            button {
                onclick: move |_| {
                    window().webview.zoom(1.0).unwrap();
                },
                "Reset zoom"
            }
            Child { a: 123, b: "hello!?" }
            Child2 {}
            Child2 {}
            // NewKid {}
            for i in 0..count() {
                div { "You wow that's insane it works {i} {i} {i * 2} times" }
                button {
                    onclick: move |_| {
                        window().webview.zoom(2.0).unwrap();
                    },
                    "Zoom in!!!"
                }
            }
        }
    }
}

//
// #[component]
// fn NewKid() -> Element {
//     rsx! {
//         div { "NewKidoo!" }
//     }
// }

static MyGlobal: GlobalSignal<i32> = GlobalSignal::new(|| 0);

struct NewStruct {
    abc: i32,
}

impl NewStruct {
    fn new() -> Self {
        Self { abc: 0 }
    }
}

#[component]
fn GlobalInner() -> Element {
    println!("TypeId: {:?}", TypeId::of::<NewStruct>());

    let s = NewStruct::new();

    rsx! {
        h1 { "GlobalSignal: {MyGlobal}" }
        h3 { "NewStruct: {s.abc}" }
        button {
            onclick: move |_| {
                *MyGlobal.write() += 1;
            },
            "Increment global"
        }
        button {
            onclick: move |_| {
                *MyGlobal.write() -= 1;
            },
            "Decrement global"
        }
    }
}

#[component]
fn Child(a: i32, b: String) -> Element {
    let mut count = use_signal(|| 2);

    rsx! {
        h1 { "Hi from child: {a} {b} -> {count}" }
        button {
            onclick: move |_| {
                count.set(count() + 1);
            },
            "Increment Count"
        }
        button {
            onclick: move |_| {
                count.set(count() - 1);
            },
            "Decrement count"
        }
        GlobalInner {}
        AddingLogger {}
    }
}

#[component]
fn Child2() -> Element {
    rsx! {
        div { "Child 4" }
        div { "Child 4" }
        div { "Child 4" }
    }
}

#[component]
fn AddingLogger() -> Element {
    let mut items = use_signal(|| vec![]);
    let mut cur_entry = use_signal(|| String::new());

    let mut add_item = move || {
        if cur_entry().is_empty() {
            return;
        }

        items.write().push(cur_entry().clone());
        cur_entry.set(String::new());
    };

    rsx! {
        div {
            input {
                placeholder: "Add an item",
                r#type: "text",
                oninput: move |evt| {
                    cur_entry.set(evt.value());
                },
                onkeypress: move |evt| {
                    if evt.key() == Key::Enter {
                        add_item();
                    }
                },
                value: "{cur_entry()}",
            }
            button {
                onclick: move |_| {
                    add_item();
                },
                "Add"
            }
            for (idx , item) in items.iter().enumerate() {
                div {
                    button {
                        onclick: move |_| {
                            items.write().remove(idx);
                        },
                        "Remove"
                    }
                    span { "{item}" }
                }
            }
        }
    }
}
