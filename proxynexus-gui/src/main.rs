use dioxus::prelude::*;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    #[cfg(feature = "desktop")]
    {
        LaunchBuilder::desktop()
            .with_cfg(
                dioxus::desktop::Config::new()
                    .with_menu(None)
                    .with_window(dioxus::desktop::WindowBuilder::new().with_title("Proxy Nexus")),
            )
            .launch(App);
    }

    #[cfg(feature = "web")]
    {
        dioxus::launch(App);
    }
}

#[component]
fn WasmSandbox() -> Element {
    use_effect(move || {
        spawn(async move {
            info!("--- STARTING WASM CORE POC ---");
            let result = test_core_function().await;
            info!("--- WASM CORE POC RESULT: {:?} ---", result);
        });
    });

    rsx! {
        div {
            class: "m-4 p-4 bg-yellow-100 border-2 border-yellow-500 rounded text-black",
            h2 { class: "font-bold", "Wasm Debug Sandbox Active" }
        }
    }
}

async fn test_core_function() -> Result<(), String> {
    Ok(())
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        if cfg!(target_arch = "wasm32") {
            WasmSandbox {}
        }

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
