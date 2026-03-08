use dioxus::prelude::*;
use proxynexus_core::db_storage::DbStorage;
use tracing::{error, info};

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");

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
        launch(App);
    }
}

fn get_db_storage() -> DbStorage {
    #[cfg(target_arch = "wasm32")]
    {
        DbStorage::new_memory()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let home = dirs::home_dir().expect("Could not find home directory");
        let db_path = home.join(".proxynexus").join("proxynexus_data");
        DbStorage::new_sled(&db_path).expect("Failed to initialize sled storage")
    }
}

#[cfg(target_arch = "wasm32")]
async fn hydrate_wasm_db(db: &mut DbStorage) -> Result<(), String> {
    use gloo_net::http::Request;

    let response = Request::get("/init.sql")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch init.sql: {}", e))?;

    if !response.ok() {
        return Err(format!(
            "Failed to fetch init.sql: HTTP {}",
            response.status()
        ));
    }

    let sql = response
        .text()
        .await
        .map_err(|e| format!("Failed to read init.sql text: {}", e))?;

    info!("Executing init.sql (size: {} bytes)...", sql.len());

    db.execute(&sql)
        .await
        .map_err(|e| format!("Hydration execution error: {}", e))?;

    info!("WASM Hydration Complete!");
    Ok(())
}

#[component]
fn App() -> Element {
    let mut db_signal = use_signal(get_db_storage);
    let mut db_ready = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            let mut db = db_signal.write();

            if let Err(e) = db.initialize_schema().await {
                error!("Schema init failed: {}", e);
            }

            #[cfg(target_arch = "wasm32")]
            {
                if let Err(e) = hydrate_wasm_db(&mut db).await {
                    error!("WASM Hydration failed: {}", e);
                }
            }

            db_ready.set(true);
        });
    });

    if !*db_ready.read() {
        return rsx! { div { class: "flex h-screen items-center justify-center bg-gray-50 text-gray-500", "Loading Database..." } };
    }

    rsx! {
        Stylesheet { href: MAIN_CSS }
        Stylesheet { href: TAILWIND_CSS }
        Workspace {}
    }
}

#[component]
fn Workspace() -> Element {
    let mut sidebar_width = use_signal(|| 300.0);
    let mut drag_state = use_signal(|| None::<(f64, f64)>);

    rsx! {
        div {
            class: "absolute inset-0 flex overflow-hidden select-none",
            onmousemove: move |evt| {
                let current_x = evt.data.client_coordinates().x;

                if let Some((start_x, start_width)) = *drag_state.read() {
                    let delta = current_x - start_x;
                    sidebar_width.set((start_width + delta).clamp(150.0, 800.0));
                }
            },
            onmouseup: move |_| {
                drag_state.set(None);
            },
            onmouseleave: move |_| {
                drag_state.set(None);
            },

            div {
                style: "width: {sidebar_width()}px;",
                class: "bg-gray-50 flex-shrink-0",
            }

            div {
                class: "w-1 cursor-col-resize bg-gray-200 hover:bg-blue-400 transition-colors flex-shrink-0 z-10",
                onmousedown: move |evt| {
                    evt.prevent_default();
                    drag_state.set(Some((evt.data.client_coordinates().x, sidebar_width())));
                },
            }

            div {
                class: "flex-1 flex flex-col bg-white min-w-0",
                textarea {
                    class: "flex-1 w-full p-4 shadow-inner outline-none resize-none",
                }
            }
        }
    }
}
