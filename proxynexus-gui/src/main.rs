use dioxus::prelude::*;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_menu(None)
                .with_window(dioxus::desktop::WindowBuilder::new().with_title("Proxy Nexus")),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        div {
            class: "w-full p-4 bg-gray-800 shadow-md border-b border-gray-700",
            div {
                class: "container mx-auto flex items-center justify-between",
                div {
                    class: "text-xl font-bold text-white hover:text-blue-400 transition-colors",
                    "ProxyNexus"
                }
            }
        }

    }
}
