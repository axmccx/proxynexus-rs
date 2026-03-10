use dioxus::prelude::*;
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::query::resolve_query_printings;
use std::time::Duration;
use tracing::{error, info};

mod components;
use components::card_input::CardInput;
use components::export_controls::ExportControls;
use components::preview_grid::PreviewGrid;
use proxynexus_core::pdf::PageSize;

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

async fn sleep(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::sleep(Duration::from_millis(ms)).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");

    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::wry::http::{Response, status::StatusCode};
        use std::borrow::Cow;

        LaunchBuilder::desktop()
            .with_cfg(
                dioxus::desktop::Config::new()
                    .with_menu(None)
                    .with_window(dioxus::desktop::WindowBuilder::new().with_title("Proxy Nexus"))
                    .with_asynchronous_custom_protocol(
                        "proxynexus",
                        |_webview_id, request, responder| {
                            tokio::spawn(async move {
                                let uri = request.uri().to_string();
                                let path_str =
                                    uri.strip_prefix("proxynexus://collections/").unwrap_or("");

                                if path_str.contains("..") || path_str.starts_with('/') {
                                    error!("Blocked suspicious local image request: {}", path_str);
                                    responder.respond(
                                        Response::builder()
                                            .status(StatusCode::FORBIDDEN)
                                            .body(Cow::Borrowed("403 - Forbidden".as_bytes()))
                                            .unwrap(),
                                    );
                                    return;
                                }

                                let home = dirs::home_dir().expect("Could not find home directory");
                                let full_path =
                                    home.join(".proxynexus").join("collections").join(path_str);

                                match tokio::fs::read(&full_path).await {
                                    Ok(bytes) => {
                                        responder.respond(
                                            Response::builder()
                                                .status(StatusCode::OK)
                                                .header("Content-Type", "image/jpeg")
                                                .header("Access-Control-Allow-Origin", "*")
                                                .body(Cow::Owned(bytes))
                                                .unwrap(),
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to load local image {}: {}",
                                            full_path.display(),
                                            e
                                        );
                                        responder.respond(
                                            Response::builder()
                                                .status(StatusCode::NOT_FOUND)
                                                .body(Cow::Borrowed("404 - Not Found".as_bytes()))
                                                .unwrap(),
                                        );
                                    }
                                }
                            });
                        },
                    ),
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
        Workspace { db_signal }
    }
}

#[component]
fn Workspace(db_signal: Signal<DbStorage>) -> Element {
    let mut sidebar_width = use_signal(|| 400.0);
    let mut drag_state = use_signal(|| None::<(f64, f64)>);

    let immediate_text = use_signal(String::new);
    let mut debounced_text = use_signal(String::new);
    let mut debounce_task = use_signal(|| None::<dioxus::dioxus_core::Task>);

    use_effect(move || {
        let current_text = immediate_text();

        if let Some(task) = debounce_task.take() {
            task.cancel();
        }

        debounce_task.set(Some(spawn(async move {
            sleep(300).await;
            debounced_text.set(current_text);
        })));
    });

    let query_result = use_resource(move || async move {
        let text = debounced_text();
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let source = proxynexus_core::card_source::Cardlist(text);
        let mut db = db_signal.write();

        resolve_query_printings(&source, &mut db).await
    });

    rsx! {
        div {
            class: "absolute inset-0 flex overflow-hidden select-none",
            onmousemove: move |evt| {
                let current_x = evt.data.client_coordinates().x;

                if let Some((start_x, start_width)) = *drag_state.read() {
                    let delta = current_x - start_x;
                    sidebar_width.set((start_width - delta).clamp(150.0, 800.0));
                }
            },
            onmouseup: move |_| {
                drag_state.set(None);
            },
            onmouseleave: move |_| {
                drag_state.set(None);
            },

            div {
                class: "flex-1 flex flex-col bg-gray-50 min-w-0 p-6 overflow-auto",
                if let Some(result) = query_result.read().as_ref() {
                    match result {
                        Ok(printings) if printings.is_empty() => rsx! {
                            div { class: "text-gray-500", "Preview of selected cards..." }
                        },
                        Ok(printings) => rsx! {
                            PreviewGrid { printings: printings.clone() }
                        },
                        Err(e) => rsx! {
                            div { class: "text-red-500 font-bold", "Error: {e}" }
                        },
                    }
                } else {
                    div { class: "text-gray-500", "Loading..." }
                }
            }

            div {
                class: "w-1 cursor-col-resize bg-gray-200 hover:bg-blue-400 transition-colors flex-shrink-0 z-10",
                onmousedown: move |evt| {
                    evt.prevent_default();
                    drag_state.set(Some((evt.data.client_coordinates().x, sidebar_width())));
                },
            }

            div {
                style: "width: {sidebar_width()}px;",
                class: "bg-white flex-shrink-0 flex flex-col h-full border-l border-gray-200",
                CardInput { text_state: immediate_text }
                ExportControls {
                    on_generate: move |page_size: PageSize| {
                        info!("Generate clicked with page size: {:?}", page_size);
                    }
                }
            }
        }
    }
}
