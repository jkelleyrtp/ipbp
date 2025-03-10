// fn zoom_controls() -> Element {
//     let mut count = use_signal(|| 0);

//     let b = 9999;

//     rsx! {
//         div {
//             h1 { "Rust VIBE CODING it!!" }
//             em { "powered by 123" }
//             h2 { "Dioxus Binary Patching 💪" }
//             button {
//                 onclick: move |_| {
//                     count.set(count() + 1);
//                 },
//                 "Click me?"
//             }
//             button {
//                 onclick: move |_| {
//                     count.set(count() + 1);
//                 },
//                 "Click me?"
//             }
//             button {
//                 onclick: move |_| {
//                     count.set(count() + 6);
//                 },
//                 "Click me again?!!!"
//                 "Click me again?!!!"
//             }
//             div { "You wow that's insane it works {count() * 6} times" }
//             button {
//                 onclick: move |_| {
//                     window().webview.zoom(1.5).unwrap();
//                 },
//                 "Zoom in"
//             }
//             button {
//                 onclick: move |_| {
//                     window().webview.zoom(45.0).unwrap();
//                 },
//                 "Reset zoom"
//             }
//             for i in 0..count() {
//                 div { "You wow that's insane it works {i} {i} {i * 2} times" }
//                 button {
//                     onclick: move |_| {
//                         window().webview.zoom(2.0).unwrap();
//                     },
//                     "Zoom in!!!"
//                     "Zoom in!!!"
//                 }
//             }
//             Child2 {}
//             NewKid {}

//             ImportChild { id: 123, opt: "hello".to_string() }

//             ImportChild { id: 123, opt: "hello".to_string() }
//             ImportChild { id: 123, opt: "hello".to_string() }

//         }
//     }
// }

// //
// #[component]
// fn NewKid() -> Element {
//     rsx! {
//         div { "NewKidoo!" }
//     }
// }

// static MyGlobal: GlobalSignal<i32> = GlobalSignal::new(|| 0);

// struct NewStruct {
//     abc: i32,
//     def: i32,
// }

// impl NewStruct {
//     fn new() -> Self {
//         Self { abc: 12, def: 0 }
//     }
// }

// #[component]
// fn GlobalInner() -> Element {
//     println!("TypeId: {:?}", TypeId::of::<NewStruct>());

//     let s = NewStruct::new();

//     rsx! {
//         h1 { "GlobalSignal: {MyGlobal}" }
//         h3 { "NewStruct: {s.abc}" }
//         h3 { "NewStruct pt2: {s.def}" }
//         button {
//             onclick: move |_| {
//                 *MyGlobal.write() += 5;
//             },
//             "Increment global"
//         }
//         button {
//             onclick: move |_| {
//                 *MyGlobal.write() -= 1;
//             },
//             "Decrement global"
//         }
//         Child { a: 123, b: "hello!?" }
//     }
// }

// #[component]
// fn Child(a: i32, b: String) -> Element {
//     let mut count = use_signal(|| 2);

//     rsx! {
//         h1 { "Hi from child: {a} {b} -> {count}" }
//         button {
//             onclick: move |_| {
//                 count.set(count() + 1);
//             },
//             "Increment Count"
//         }
//         button {
//             onclick: move |_| {
//                 count.set(count() - 1);
//             },
//             "Decrement count"
//         }
//         div { "---------------------------------" }
//         Child3 {}
//         div { "---------------------------------" }
//         GlobalInner {}
//         div { "---------------------------------" }
//         AddingLogger {}
//     }
// }

// #[component]
// fn Child3() -> Element {
//     rsx! {
//         div { "Child 3" }
//     }
// }

// #[component]
// fn Child2() -> Element {
//     rsx! {
//         div { "Child 4" }
//     }
// }

// #[component]
// fn AddingLogger() -> Element {
//     let mut items = use_signal(|| vec![]);
//     let mut cur_entry = use_signal(|| String::new());

//     let mut add_item = move || {
//         if cur_entry().is_empty() {
//             return;
//         }

//         items.write().push(cur_entry().clone());
//         cur_entry.set(String::new());
//     };

//     rsx! {
//         div {
//             input {
//                 placeholder: "Add an item",
//                 r#type: "text",
//                 oninput: move |evt| {
//                     cur_entry.set(evt.value());
//                 },
//                 onkeypress: move |evt| {
//                     if evt.key() == Key::Enter {
//                         add_item();
//                     }
//                 },
//                 value: "{cur_entry()}",
//             }
//             button {
//                 onclick: move |_| {
//                     add_item();
//                 },
//                 "Add"
//             }
//             for (idx , item) in items.iter().enumerate() {
//                 div {
//                     button {
//                         onclick: move |_| {
//                             items.write().remove(idx);
//                         },
//                         "Remove"
//                     }
//                     span { "{item}" }
//                 }
//             }
//         }
//         ImportApp {}
//     }
// }

// fn ImportApp() -> Element {
//     let mut count = use_signal(|| 0);
//     let abcv = 99111;

//     rsx! {
//         h1 { "{count}" }
//         button {
//             onclick: move |_| {
//                 count.set(count() + 1);
//             },
//             "Increment {abcv}"
//         }
//         button {
//             onclick: move |_| {
//                 count.set(count() + 1);
//             },
//             "Increment {abcv}"
//         }
//         button {
//             onclick: move |_| {
//                 count.set(count() + 1);
//             },
//             "Increment {abcv}"
//         }
//     }
// }

// #[component]
// fn ImportChild(id: u32, opt: String) -> Element {
//     rsx! {
//         div { "Hello ?? child: {id} - {opt} ?" }
//     }
// }
