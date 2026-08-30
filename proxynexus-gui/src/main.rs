#![allow(clippy::await_holding_invalid_type)]

use dioxus::prelude::*;
use proxynexus_core::card_source::{Cardlist, DecklistUrl, SetName};
use proxynexus_core::card_store::normalize_title;
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::models::{Printing, ResolvedPrintings};
use proxynexus_core::query::{apply_variant_overrides, resolve_query_printings};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{error, info};
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

pub mod analytics;
mod components;
mod export;
use components::about_modal::AboutModal;
use components::export_controls::ExportControls;
use components::preview_grid::PreviewGrid;
use components::print_layout_info::PrintLayoutInfo;
use components::sides_info::SidesInfo;
use components::source_selector::{ActiveSource, SourceSelector};
use components::upscale_info::UpscaleInfo;
use components::variant_selector::{VariantSelector, VariantSelectorState};

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

pub static GPU_AVAILABLE: GlobalSignal<bool> = Signal::global(|| false);

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
        info!("Analytics disabled: POSTHOG_API_KEY not set");
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
                            .with_inner_size(dioxus::desktop::LogicalSize::new(1850.0, 1400.0)),
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
        #[cfg(target_arch = "wasm32")]
        {
            if web_sys::window().is_none() {
                // So not start the Dioxus UI in a web worker
                return;
            }
        }
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
async fn hydrate_wasm_db(db: &mut DbStorage) -> anyhow::Result<()> {
    use anyhow::{Context, anyhow};
    use flate2::read::GzDecoder;
    use gloo_net::http::Request;
    use std::io::Read;

    let url = format!("/init.sql.gz?t={}", js_sys::Date::now());
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch init.sql.gz: {}", e))?;

    if !response.ok() {
        return Err(anyhow!(
            "Failed to fetch init.sql.gz: HTTP {}",
            response.status()
        ));
    }

    let bytes = response
        .binary()
        .await
        .map_err(|e| anyhow!("Failed to read init.sql.gz binary data: {}", e))?;

    info!(
        "Decompressing init.sql.gz (compressed size: {} bytes)...",
        bytes.len()
    );

    let mut decoder = GzDecoder::new(&bytes[..]);
    let mut sql = String::new();
    decoder
        .read_to_string(&mut sql)
        .map_err(|e| anyhow!("Failed to decompress init.sql: {}", e))?;

    info!(
        "Executing init.sql (decompressed size: {} bytes)...",
        sql.len()
    );

    db.execute(&sql)
        .await
        .context("Hydration execution error")?;

    info!("WASM Hydration Complete!");
    Ok(())
}

use async_lock::Mutex;
use std::sync::Arc;

#[component]
fn App() -> Element {
    let db_signal = use_signal(|| Arc::new(Mutex::new(get_db_storage())));
    let mut db_ready = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            let db_arc = db_signal.read().clone();
            let mut db = db_arc.lock().await;

            if let Err(e) = db.initialize_schema().await {
                error!("Schema init failed: {:?}", e);
            }

            #[cfg(target_arch = "wasm32")]
            {
                if let Err(e) = hydrate_wasm_db(&mut db).await {
                    error!("WASM Hydration failed: {:?}", e);
                }
            }

            *GPU_AVAILABLE.write() = proxynexus_core::probe_gpu().await;

            db_ready.set(true);
        });
    });

    rsx! {
        Stylesheet { href: MAIN_CSS }
        Stylesheet { href: TAILWIND_CSS }

        if *db_ready.read() {
            Workspace { db_signal }
        }
    }
}

fn copy_selection_to_clipboard(game_id: &str, printings: &[Printing]) {
    let mut list = String::new();
    let mut current_title = String::new();
    let mut current_count = 0;
    let mut current_variant = String::new();

    let mut flush = |count: usize, title: &str, variant: &str| {
        if count > 0 {
            list.push_str(&format!("{}x {} [{}]\n", count, title, variant));
        }
    };

    for p in printings {
        let variant_str = p.variant_key();

        if p.card_title == current_title && variant_str == current_variant {
            current_count += 1;
        } else {
            flush(current_count, &current_title, &current_variant);
            current_title = p.card_title.clone();
            current_variant = variant_str;
            current_count = 1;
        }
    }
    flush(current_count, &current_title, &current_variant);

    let escaped_list = list
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('$', "\\$");

    let eval_str = format!(
        "
        const list = `{}`;
        const url = 'https://proxynexus.net/?v=1&game={}&list=' + encodeURIComponent(list);
        navigator.clipboard.writeText(url);
        ",
        escaped_list, game_id
    );
    let _ = dioxus::document::eval(&eval_str);
}

#[component]
fn Workspace(db_signal: Signal<Arc<Mutex<DbStorage>>>) -> Element {
    let progress = use_signal(|| None::<f32>);

    let (url_game, url_list, subdomain_game) = {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let location = window.location();

                let subdomain_game = location.hostname().ok().and_then(|hostname| {
                    let parts: Vec<&str> = hostname.split('.').collect();
                    if parts.len() >= 2 {
                        let subdomain = parts[0];
                        if subdomain != "www" && subdomain != "proxynexus" {
                            proxynexus_core::games::get_game_id_by_subdomain(subdomain)
                                .map(|s| s.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });

                let params = location
                    .search()
                    .ok()
                    .and_then(|s| web_sys::UrlSearchParams::new_with_str(&s).ok())
                    .filter(|p| p.get("v") == Some("1".to_string()));

                let game = params.as_ref().and_then(|p| p.get("game"));
                let list = params.as_ref().and_then(|p| p.get("list"));
                (game, list, subdomain_game)
            } else {
                (None, None, None)
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        (None::<String>, None::<String>, None::<String>)
    };

    let mut active_game_id = use_signal(|| {
        if url_game.is_some() {
            return url_game;
        }

        if subdomain_game.is_some() {
            return subdomain_game;
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(local_storage)) = window.local_storage() {
                    if let Ok(Some(saved)) = local_storage.get_item("active_game_id") {
                        if !saved.trim().is_empty() {
                            return Some(saved);
                        }
                    }
                }
            }
        }
        None::<String>
    });

    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(local_storage)) = window.local_storage() {
                    if let Some(id) = active_game_id() {
                        let _ = local_storage.set_item("active_game_id", &id);
                    } else {
                        let _ = local_storage.remove_item("active_game_id");
                    }
                }
            }
        }
    });

    let available_games = use_resource(move || async move {
        let db_arc = db_signal.read().clone();
        let mut db = db_arc.lock().await;
        db.get_games().await.unwrap_or_default()
    });

    let active_source = use_signal(|| {
        if let Some(list) = url_list {
            return ActiveSource::Cardlist(list);
        }
        ActiveSource::default()
    });
    let active_card_name = use_signal(|| None::<String>);
    let mut debounced_active_card_name = use_signal(|| None::<String>);
    let mut debounced_source = use_signal(ActiveSource::default);
    let mut debounce_task = use_signal(|| None::<dioxus_core::Task>);

    let mut global_overrides = use_signal(HashMap::<String, String>::new);
    let mut index_overrides = use_signal(HashMap::<(String, usize), String>::new);

    let mut open_variant_selector = use_signal(|| None::<VariantSelectorState>);
    let mut is_overrides_reset_pending = use_signal(|| false);
    let mut is_about_open = use_signal(|| false);
    let mut show_copy_toast = use_signal(|| false);
    let mut print_layout_info_pos = use_signal(|| None::<(f64, f64, f64)>);
    let mut sides_info_pos = use_signal(|| None::<(f64, f64, f64)>);
    let mut upscale_info_pos = use_signal(|| None::<(f64, f64, f64)>);

    use_effect(move || {
        let current_source = active_source();
        let current_active_card = active_card_name();

        if let Some(task) = debounce_task.take() {
            task.cancel();
        }

        match current_source {
            ActiveSource::Cardlist(_) => {
                debounce_task.set(Some(spawn(async move {
                    sleep(300).await;
                    debounced_source.set(current_source);
                    debounced_active_card_name.set(current_active_card);
                })));
            }
            _ => {
                debounced_source.set(current_source);
                debounced_active_card_name.set(current_active_card);
            }
        }
    });

    let raw_data_resource = use_resource(move || async move {
        let game_id = match active_game_id() {
            Some(id) => id,
            None => return Ok(ResolvedPrintings::default()),
        };

        let source = debounced_source();
        let db_arc = db_signal.read().clone();
        let mut db = db_arc.lock().await;

        let res = match source {
            ActiveSource::Cardlist(text) => {
                if text.trim().is_empty() {
                    return Ok(ResolvedPrintings::default());
                }
                resolve_query_printings(&Cardlist(text), &mut db, &game_id)
                    .await
                    .map_err(anyhow::Error::from)
            }
            ActiveSource::SetName(name) => {
                if name.trim().is_empty() {
                    return Ok(ResolvedPrintings::default());
                }
                resolve_query_printings(&SetName(name), &mut db, &game_id)
                    .await
                    .map_err(anyhow::Error::from)
            }
            ActiveSource::DecklistUrl(url) => {
                if url.trim().is_empty() {
                    return Ok(ResolvedPrintings::default());
                }
                resolve_query_printings(&DecklistUrl(url), &mut db, &game_id)
                    .await
                    .map_err(anyhow::Error::from)
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
            Ok(result) => {
                let applied = apply_variant_overrides(
                    &result.printings,
                    &result.available_variants,
                    &global_overrides.read(),
                    &index_overrides.read(),
                );
                Some(Ok((
                    result.printings.clone(),
                    applied,
                    result.available_variants.clone(),
                    result.not_found.clone(),
                )))
            }
            Err(e) => Some(Err(format!("{:?}", e))),
        }
    });

    let back_labels = use_resource(move || async move {
        let Some(game_id) = active_game_id() else {
            return Vec::new();
        };
        let printings = match ordered_printings
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
        {
            Some((_, printings, _, _)) => printings.clone(),
            None => Vec::new(),
        };

        let db_arc = db_signal.read().clone();
        let mut db = db_arc.lock().await;
        proxynexus_core::card_backs::allowed_labels(&mut db, &game_id, &printings)
            .await
            .unwrap_or_default()
    });

    let printings_by_title = use_memo(move || {
        let res = ordered_printings.read();
        let (_base, printings, available, _not_found) = res.as_ref()?.as_ref().ok()?;

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
        let (x, y, w, h) = state.rect;
        let desk_left = x + w + 8.0;
        let desk_top = y;
        let mob_left = x;
        let mob_top = y + h + 8.0;

        if let Some((grouped, available)) = printings_by_title.read().as_ref() {
            let title_norm = state.id.0.clone();
            let occurrence = state.id.1;

            if let Some(group) = grouped.get(&title_norm) {
                if let Some(printing) = group.get(occurrence) {
                    if let Some(variants) = available.get(&title_norm) {
                        let total_copies = group.len();

                        rsx! {
                            div {
                                class: "absolute pointer-events-auto z-[1000] top-[var(--mob-top)] left-[var(--mob-left)] md:top-[var(--desk-top)] md:left-[var(--desk-left)] [transform:translateX(min(0px,calc(100vw-1rem-var(--mob-left)-100%)))] md:[transform:translateX(min(0px,calc(100vw-1rem-var(--desk-left)-100%)))]",
                                style: "--desk-top: {desk_top}px; --desk-left: {desk_left}px; --mob-top: {mob_top}px; --mob-left: {mob_left}px;",
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

    let is_generate_disabled = match ordered_printings.read().as_ref() {
        Some(Ok((_, applied, _, _))) => applied.is_empty(),
        _ => true,
    };

    rsx! {
        div {
            class: "absolute inset-0 flex flex-col md:flex-row bg-gray-50",
            onclick: move |_| open_variant_selector.set(None),
            onwheel: move |_| open_variant_selector.set(None),

            div {
                class: "flex-1 flex flex-col min-w-0 min-h-0 p-4 md:p-6 overflow-y-auto",
                style: "z-index: 20;",
                if let Some(result) = ordered_printings.read().as_ref() {
                    match result {
                        Ok((_, printings, _, not_found)) if printings.is_empty() && not_found.is_empty() => rsx! {
                            div { class: "text-gray-500", "Preview of selected cards..." }
                        },
                        Ok((base_printings, printings, available, not_found)) => {
                            let display_not_found: Vec<_> = not_found.iter().filter(|&n| {
                                if let Some(active) = debounced_active_card_name() {
                                    n != &active
                                } else {
                                    true
                                }
                            }).collect();

                            rsx! {
                                if !display_not_found.is_empty() {
                                    div {
                                        class: "bg-yellow-50 text-yellow-800 p-4 rounded-md mb-4 font-semibold shadow-sm border border-yellow-200",
                                        "The following card(s) were not found:"
                                        ul {
                                            class: "list-disc list-inside mt-2 font-normal text-yellow-900 font-mono text-sm",
                                            for missing in display_not_found {
                                                li { "{missing}" }
                                            }
                                        }
                                    }
                                }
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
                style: "z-index: 10;",
                class: "relative md:w-[440px] bg-white flex-shrink-0 flex flex-col border-t md:border-t-0 md:border-l border-gray-200",
                div {
                    class: "p-2 border-b border-gray-200 shrink-0 flex items-center justify-between gap-3",
                    select {
                        class: "w-auto max-w-full p-1.5 border border-gray-300 rounded-md outline-none focus:ring-2 focus:ring-blue-400 bg-white text-sm",
                        value: active_game_id().unwrap_or_default(),
                        onchange: move |evt| {
                            active_game_id.set(Some(evt.value()));
                            debounced_source.set(ActiveSource::default());
                            is_overrides_reset_pending.set(true);
                        },
                        if active_game_id().is_none() {
                            option { value: "", disabled: true, selected: true, "Select a game..." }
                        }
                        if let Some(games) = available_games.read().as_ref() {
                            for (id, display_name) in games {
                                option {
                                    value: id.clone(),
                                    selected: Some(id.clone()) == active_game_id(),
                                    "{display_name}"
                                }
                            }
                        }
                    }
                    div {
                        class: "flex items-center gap-2",
                        if let Some(Ok((_, resolved_printing, _, _))) = ordered_printings.read().as_ref() {
                            if !resolved_printing.is_empty() {
                                button {
                                    class: "px-2 py-1 text-sm font-medium text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 flex-shrink-0 whitespace-nowrap",
                                    onclick: move |_| {
                                        if let Some(game_id) = active_game_id() {
                                            let printings_clone = if let Some(Ok((_, resolved_printing, _, _))) = ordered_printings.read().as_ref() {
                                                resolved_printing.clone()
                                            } else {
                                                Vec::new()
                                            };

                                            copy_selection_to_clipboard(&game_id, &printings_clone);
                                            analytics::send_event("copy_link_clicked");
                                            show_copy_toast.set(true);
                                            spawn(async move {
                                                sleep(2000).await;
                                                show_copy_toast.set(false);
                                            });
                                        }
                                    },
                                    title: "Copy a link to the current card and variant selection",
                                    "Copy Link"
                                }
                            }
                        }
                        button {
                            class: "px-2 py-1 text-sm font-medium text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 flex-shrink-0 whitespace-nowrap",
                            onclick: move |_| {
                                analytics::send_event("about_clicked");
                                is_about_open.set(true);
                            },
                            title: "About Proxy Nexus",
                            "About"
                        }
                    }
                }

                SourceSelector {
                    active_game_id,
                    source_state: active_source,
                    db_signal,
                    active_card_name,
                    on_source_changed: move |_| {
                        is_overrides_reset_pending.set(true);
                    }
                }
                ExportControls {
                    progress,
                    is_disabled: is_generate_disabled,
                    on_open_info: move |pos| print_layout_info_pos.set(Some(pos)),
                    on_open_upscale_info: move |pos| upscale_info_pos.set(Some(pos)),
                    on_open_sides_info: move |pos| sides_info_pos.set(Some(pos)),
                    back_labels,
                    on_generate: move |options: export::ExportOptions| {
                        let source = active_source();
                        if let Some(game_id) = active_game_id() {
                            spawn(export::run_export(
                                db_signal,
                                game_id,
                                source,
                                options,
                                progress,
                                global_overrides.read().clone(),
                                index_overrides.read().clone(),
                            ));
                        }
                    }
                }
            }

            {variant_selector_overlay}

            if is_about_open() {
                AboutModal {
                    on_close: move |_| is_about_open.set(false),
                }
            }

            if let Some(pos) = print_layout_info_pos() {
                PrintLayoutInfo {
                    pos,
                    on_close: move |_| print_layout_info_pos.set(None),
                }
            }

            if let Some(pos) = sides_info_pos() {
                SidesInfo {
                    pos,
                    on_close: move |_| sides_info_pos.set(None),
                }
            }

            if let Some(pos) = upscale_info_pos() {
                UpscaleInfo {
                    pos,
                    on_close: move |_| upscale_info_pos.set(None),
                }
            }

            if show_copy_toast() {
                div {
                    class: "fixed top-4 right-4 bg-green-500/90 text-white px-4 py-2 rounded-md shadow-lg z-[100] transition-all duration-300 pointer-events-none font-medium backdrop-blur-sm",
                    "Link copied to clipboard!"
                }
            }
        }
    }
}
