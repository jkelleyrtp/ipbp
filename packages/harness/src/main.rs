use dioxus::prelude::*;

mod hooks;
pub use hooks::*;

fn main() {
    dioxus::launch(app);
}

fn app() -> Element {
    rsx! {
        div {
            h1 { "Hotrleoad" }
        }
    }
}
