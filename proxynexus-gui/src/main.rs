use dioxus::prelude::*;
use proxynexus_core::card_source::{Cardlist, NrdbUrl, SetName};
use proxynexus_core::card_store::normalize_title;
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::models::Printing;
use proxynexus_core::query::{apply_variant_overrides, resolve_query_printings};
use std::collections::HashMap;
use std::time::Duration;
use tracing::error;
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

pub mod analytics;
mod components;
mod export;
use components::export_controls::ExportControls;
use components::preview_grid::PreviewGrid;
use components::source_selector::{ActiveSource, SourceSelector};
use components::variant_selector::{VariantSelector, VariantSelectorState};

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

fn init_tracing() {
    let registry = tracing_subscriber::registry();

    #[cfg(target_arch = "wasm32")]
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse_lossy("proxynexus=debug,proxynexus_gui=debug,proxynexus_core=debug");

    #[cfg(not(target_arch = "wasm32"))]
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let registry = registry.with(filter);

    #[cfg(target_arch = "wasm32")]
    let registry = registry.with(tracing_wasm::WASMLayer::new(
        tracing_wasm::WASMLayerConfig::default(),
    ));

    #[cfg(not(target_arch = "wasm32"))]
    let registry = registry.with(tracing_subscriber::fmt::layer());

    if analytics::is_enabled() {
        let _ = registry.with(analytics::LogCaptureLayer).try_init();
    } else {
        let _ = registry.try_init();
    }
}

fn main() {
    init_tracing();

    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::wry::http::{Response, status::StatusCode};
        use std::borrow::Cow;

        LaunchBuilder::desktop()
            .with_cfg(
                dioxus::desktop::Config::new()
                    .with_menu(None)
                    .with_window(
                        dioxus::desktop::WindowBuilder::new()
                            .with_title("Proxy Nexus")
                            .with_inner_size(dioxus::desktop::LogicalSize::new(1200.0, 1000.0)),
                    )
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
                                        let content_type =
                                            if full_path.extension().and_then(|e| e.to_str())
                                                == Some("png")
                                            {
                                                "image/png"
                                            } else {
                                                "image/jpeg"
                                            };
                                        responder.respond(
                                            Response::builder()
                                                .status(StatusCode::OK)
                                                .header("Content-Type", content_type)
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

    let url = format!("/init.sql?t={}", js_sys::Date::now());
    let response = Request::get(&url)
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

    db.execute(&sql)
        .await
        .map_err(|e| format!("Hydration execution error: {}", e))?;

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
    let progress = use_signal(|| None::<f32>);

    let active_source = use_signal(ActiveSource::default);
    let mut debounced_source = use_signal(ActiveSource::default);
    let mut debounce_task = use_signal(|| None::<dioxus_core::Task>);

    let mut global_overrides = use_signal(HashMap::<String, String>::new);
    let mut index_overrides = use_signal(HashMap::<(String, usize), String>::new);

    let mut open_variant_selector = use_signal(|| None::<VariantSelectorState>);
    let mut is_overrides_reset_pending = use_signal(|| false);

    use_effect(move || {
        let current_source = active_source();

        if let Some(task) = debounce_task.take() {
            task.cancel();
        }

        match current_source {
            ActiveSource::Cardlist(_) => {
                debounce_task.set(Some(spawn(async move {
                    sleep(300).await;
                    debounced_source.set(current_source);
                })));
            }
            _ => {
                debounced_source.set(current_source);
            }
        }
    });

    let raw_data_resource = use_resource(move || async move {
        let source = debounced_source();
        let mut db = db_signal.write();

        let res = match source {
            ActiveSource::Cardlist(text) => {
                if text.trim().is_empty() {
                    return Ok((Vec::new(), HashMap::new()));
                }
                resolve_query_printings(&Cardlist(text), &mut db).await
            }
            ActiveSource::SetName(name) => {
                if name.trim().is_empty() {
                    return Ok((Vec::new(), HashMap::new()));
                }
                resolve_query_printings(&SetName(name), &mut db).await
            }
            ActiveSource::NrdbUrl(url) => {
                if url.trim().is_empty() {
                    return Ok((Vec::new(), HashMap::new()));
                }
                resolve_query_printings(&NrdbUrl(url), &mut db).await
            }
        };

        if *is_overrides_reset_pending.peek() {
            global_overrides.write().clear();
            index_overrides.write().clear();
            is_overrides_reset_pending.set(false);
        }

        res
    });

    let ordered_printings = use_memo(move || {
        let res = raw_data_resource.read();
        let res_val = res.as_ref()?;

        match res_val {
            Ok((base, available)) => {
                let applied = apply_variant_overrides(
                    base,
                    available,
                    &global_overrides.read(),
                    &index_overrides.read(),
                );
                Some(Ok((base.clone(), applied, available.clone())))
            }
            Err(e) => Some(Err(e.to_string())),
        }
    });

    let printings_by_title = use_memo(move || {
        let res = ordered_printings.read();
        let (_base, printings, available) = res.as_ref()?.as_ref().ok()?;

        let mut grouped = HashMap::<String, Vec<Printing>>::new();
        for p in printings {
            grouped
                .entry(normalize_title(&p.card_title))
                .or_default()
                .push(p.clone());
        }
        Some((grouped, available.clone()))
    });

    let variant_selector_overlay = if let Some(state) = open_variant_selector() {
        let (x, y, w, _h) = state.rect;
        let left = x + w + 8.0;

        if let Some((grouped, available)) = printings_by_title.read().as_ref() {
            let title_norm = state.id.0.clone();
            let occurrence = state.id.1;

            if let Some(group) = grouped.get(&title_norm) {
                if let Some(printing) = group.get(occurrence) {
                    if let Some(variants) = available.get(&title_norm) {
                        let total_copies = group.len();

                        rsx! {
                            div {
                                class: "absolute pointer-events-auto z-[1000]",
                                style: "top: {y}px; left: {left}px;",
                                onclick: move |evt| evt.stop_propagation(),
                                VariantSelector {
                                    printing: printing.clone(),
                                    variants: variants.clone(),
                                    total_copies,
                                    on_close: move |_| open_variant_selector.set(None),
                                    on_override: move |(apply_to_all, variant_str): (bool, String)| {
                                        let normalized = title_norm.clone();
                                        if apply_to_all {
                                            global_overrides.write().insert(normalized.clone(), variant_str);
                                            index_overrides.write().retain(|(t, _), _| t != &normalized);
                                            open_variant_selector.set(None);
                                        } else {
                                            index_overrides.write().insert((normalized, occurrence), variant_str);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! { "" }
                    }
                } else {
                    rsx! { "" }
                }
            } else {
                rsx! { "" }
            }
        } else {
            rsx! { "" }
        }
    } else {
        rsx! { "" }
    };

    rsx! {
        div {
            class: "absolute inset-0 flex overflow-hidden select-none bg-gray-50",
            onclick: move |_| open_variant_selector.set(None),
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
            onwheel: move |_| open_variant_selector.set(None),

            div {
                class: "flex-1 flex flex-col min-w-0 p-6 overflow-y-auto",
                style: "z-index: 20;",
                if let Some(result) = ordered_printings.read().as_ref() {
                    match result {
                        Ok((_, printings, _)) if printings.is_empty() => rsx! {
                            div { class: "text-gray-500", "Preview of selected cards..." }
                        },
                        Ok((base_printings, printings, available)) => {
                            rsx! {
                                PreviewGrid {
                                    base_printings: base_printings.clone(),
                                    printings: printings.clone(),
                                    available_variants: available.clone(),
                                    open_variant_selector,
                                }
                            }
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
                class: "h-full w-1 cursor-col-resize bg-gray-200 hover:bg-blue-400 transition-colors flex-shrink-0",
                style: "z-index: 15;",
                onmousedown: move |evt| {
                    evt.prevent_default();
                    drag_state.set(Some((evt.data.client_coordinates().x, sidebar_width())));
                },
            }

            div {
                style: "width: {sidebar_width()}px; z-index: 10;",
                class: "h-full bg-white flex-shrink-0 flex flex-col border-l border-gray-200",
                SourceSelector {
                    source_state: active_source,
                    db_signal,
                    on_source_changed: move |_| {
                        is_overrides_reset_pending.set(true);
                    }
                }
                ExportControls {
                    progress,
                    on_generate: move |config: components::export_controls::ExportConfig| {
                        let source = active_source();
                        spawn(export::run_export(
                            db_signal,
                            source,
                            config,
                            progress,
                            global_overrides.read().clone(),
                            index_overrides.read().clone(),
                        ));
                    }
                }
            }

            {variant_selector_overlay}
        }
    }
}
