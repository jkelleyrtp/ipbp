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
            h1 { "Rust VIBE CODING" }
            em { "powered by" }
            h2 { "Dioxus Binary Patching 💪" }
            div { "It works! Fantastically if you ask me" }
            button {
                onclick: move |_| {
                    count.set(count() + 1);
                },
                "Click me"
            }
            div { "You wow that's insane it works {count() * 2} times" }
            button {
                onclick: move |_| {
                    window().webview.zoom(1.0).unwrap();
                },
                "Reset zoom"
            }
            Child { a: 123, b: "hello!" }
            Child2 {}
            for i in 0..count() {
                div { "You wow that's insane it works {i} times" }
                button {
                    onclick: move |_| {
                        window().webview.zoom(2.0).unwrap();
                    },
                    "Zoom in"
                }
            }
        }
    }
}

#[component]
fn Child(a: i32, b: String) -> Element {
    let mut count = use_signal(|| 0);
    use_effect(move || {
        println!("count is {count}");
    });

    rsx! {
        h1 { "Hi from child: {a} {b}" }
        button {
            onclick: move |_| {
                count.set(count() + 1);
            },
            "Increment"
        }
        AddingLogger {}
        AddingLogger {}
        AddingLogger {}
    }
}

fn Child2() -> Element {
    rsx! { "Child 2" }
}

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
