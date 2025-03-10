fn app() -> Element {
    let mut count = use_signal(|| 0);
    let abcv = 91;

    rsx! {
        h1 { "{count}" }
        button {
            onclick: move |_| {
                count.set(count() + 1);
            },
            "Increment {abcv}"
        }
        button {
            onclick: move |_| {
                count.set(count() + 1);
            },
            "Increment {abcv}"
        }
        button {
            onclick: move |_| {
                count.set(count() + 1);
            },
            "Increment {abcv}"
            "Increment {abcv}"
        }
        div { "hello world!" }
        Child { id: 123, opt: "hell123o".to_string() }
        Child2 { id: 123, opt: "hello".to_string() }
        Child3 { id: 123, opt: "hello".to_string() }
    }
}

#[component]
fn Child(id: u32, opt: String) -> Element {
    rsx! {
        div { "Hello ?? child: {id} - {opt} ?" }
    }
}
#[component]
fn Child3(id: u32, opt: String) -> Element {
    rsx! {
        div { "Hello ?? child: {id} - {opt} ?" }
    }
}

#[component]
fn Child2(id: u32, opt: String) -> Element {
    rsx! {
        div { "oh lordy!" }
        div { "Hello ?? child2s: {id} - {opt} ?" }
    }
}
