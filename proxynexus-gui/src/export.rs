#![allow(clippy::await_holding_invalid_type)]
use crate::analytics;
use crate::components::source_selector::ActiveSource;
use anyhow::Context;
use async_lock::Mutex;
use dioxus::prelude::*;
use proxynexus_core::card_source::{CardSource, Cardlist, DecklistUrl, SetName};
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::games::get_card_back_adapter;
use proxynexus_core::mpc::{MpcOptions, generate_mpc_zip};
use proxynexus_core::pdf::{PdfOptions, generate_pdf};
use proxynexus_core::query::apply_variant_overrides;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{error, info};
use web_time::Instant;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum ExportOptions {
    Pdf(PdfOptions),
    Mpc(MpcOptions),
}

struct ExportMeta {
    format: &'static str,
    options: ExportOptions,
    filename: &'static str,
    filter: &'static str,
    ext: &'static str,
    mime: &'static str,
}

pub async fn run_export(
    db_signal: Signal<Arc<Mutex<DbStorage>>>,
    active_game_id: String,
    active_source: ActiveSource,
    options: ExportOptions,
    mut progress_signal: Signal<Option<f32>>,
    global_overrides: HashMap<String, String>,
    index_overrides: HashMap<(String, usize), String>,
) {
    analytics::start_capture();
    let start_time = Instant::now();
    progress_signal.set(Some(0.0));

    let atomic_progress = Arc::new(AtomicU32::new(0));
    let atomic_progress_clone = atomic_progress.clone();

    // Background task to update the UI signal from the atomic value
    let mut update_task = Some(spawn(async move {
        loop {
            let val = atomic_progress_clone.load(Ordering::Relaxed);
            let p = val as f32 / 1000.0;
            progress_signal.set(Some(p));
            if val >= 1000 {
                break;
            }

            #[cfg(not(target_arch = "wasm32"))]
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::sleep(std::time::Duration::from_millis(16)).await;
        }
    }));

    let meta = match options.clone() {
        ExportOptions::Pdf(pdf_opts) => ExportMeta {
            format: "pdf",
            options: ExportOptions::Pdf(pdf_opts),
            filename: "proxynexus_export.pdf",
            filter: "PDF Document",
            ext: "pdf",
            mime: "application/pdf",
        },
        ExportOptions::Mpc(mpc_opts) => ExportMeta {
            format: "mpc",
            options: ExportOptions::Mpc(mpc_opts),
            filename: "proxynexus_export.zip",
            filter: "ZIP Archive",
            ext: "zip",
            mime: "application/zip",
        },
    };

    info!("Starting {} export", meta.format);

    let progress_callback = Some(Box::new(move |p: f32| {
        atomic_progress.store((p * 1000.0) as u32, Ordering::Relaxed);
    }) as Box<dyn Fn(f32) + Send + Sync>);

    let (source_text, source_type) = match &active_source {
        ActiveSource::Cardlist(text) => (text.clone(), "Cardlist"),
        ActiveSource::SetName(name) => (name.clone(), "SetName"),
        ActiveSource::DecklistUrl(url) => (url.clone(), "DecklistUrl"),
    };

    let resolved_printings = async {
        let db_arc = db_signal.read().clone();
        let mut db = db_arc.lock().await;
        let mut store =
            proxynexus_core::card_store::CardStore::new(&mut db, active_game_id.clone())
                .context("Failed to create store")?;

        let reqs = match active_source {
            ActiveSource::Cardlist(text) => Cardlist(text)
                .to_card_requests(&mut store)
                .await
                .context("Failed to parse cardlist")?,
            ActiveSource::SetName(name) => SetName(name)
                .to_card_requests(&mut store)
                .await
                .context("Failed to get set cards")?,
            ActiveSource::DecklistUrl(url) => DecklistUrl(url)
                .to_card_requests(&mut store)
                .await
                .context("Failed to fetch deck from Decklist API")?,
        };

        let available = store
            .get_available_printings(&reqs)
            .await
            .context("Failed to get available printings")?;
        let base = store
            .resolve_printings(&reqs, &available)
            .context("Failed to resolve printings")?;

        Ok(apply_variant_overrides(
            &base,
            &available,
            &global_overrides,
            &index_overrides,
        ))
    }
    .await;

    let selected_printings = if let Ok(ref printings) = resolved_printings {
        printings
            .iter()
            .map(|p| {
                let p_display = p
                    .pack_id
                    .as_deref()
                    .or(p.variant.as_deref())
                    .unwrap_or("official");
                format!("{} [{}:{}]", p.card_title, p_display, p.collection)
            })
            .collect()
    } else {
        Vec::new()
    };

    #[cfg(not(target_arch = "wasm32"))]
    let image_provider_result = dirs::home_dir()
        .context("Could not find home directory")
        .map(|home| {
            let collections_path = home.join(".proxynexus").join("collections");
            proxynexus_core::image_provider::LocalImageProvider::new(collections_path)
        });

    #[cfg(target_arch = "wasm32")]
    let image_provider_result = Ok(proxynexus_core::image_provider::RemoteImageProvider);

    let result = match (resolved_printings, image_provider_result) {
        (Ok(printings), Ok(image_provider)) => match options {
            ExportOptions::Pdf(pdf_opts) => {
                generate_pdf(printings, &image_provider, pdf_opts, progress_callback)
                    .await
                    .context("PDF generation failed")
            }
            ExportOptions::Mpc(mpc_opts) => {
                let card_backs =
                    if let Some(card_back_adapter) = get_card_back_adapter(&active_game_id) {
                        card_back_adapter
                            .fetch_card_backs()
                            .await
                            .unwrap_or_default()
                    } else {
                        vec![]
                    };

                generate_mpc_zip(
                    printings,
                    &image_provider,
                    mpc_opts,
                    card_backs,
                    progress_callback,
                )
                .await
                .context("MPC ZIP generation failed")
            }
        },
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e),
    };

    let duration = start_time.elapsed();

    let mut success = false;
    let mut error_message = None;

    match &result {
        Ok(bytes) => {
            success = true;
            info!(
                "Successfully generated {}. Size: {} bytes. Total time: {:?}",
                meta.format,
                bytes.len(),
                duration
            );
        }
        Err(e) => {
            let msg = format!("Failed to generate {}: {:?}", meta.format, e);
            error!("{}", msg);
            error_message = Some(msg);
        }
    }

    if let Some(task) = update_task.take() {
        task.cancel();
    }

    analytics::send_report(analytics::GenerationReport {
        format: meta.format.to_string(),
        options: meta.options,
        runtime_ms: start_time.elapsed().as_millis(),
        success,
        active_game_id,
        source_type,
        source_text,
        selected_printings,
        error_message,
    });

    if let Ok(bytes) = result
        && let Err(e) = save_file(&bytes, meta.filename, meta.filter, meta.ext, meta.mime).await
    {
        error!("Failed to save {}: {:?}", meta.format, e);
    }

    progress_signal.set(None);
}

#[cfg(not(target_arch = "wasm32"))]
async fn save_file(
    bytes: &[u8],
    file_name: &str,
    filter_name: &str,
    extension: &str,
    _mime_type: &str,
) -> anyhow::Result<()> {
    if let Some(path) = rfd::AsyncFileDialog::new()
        .add_filter(filter_name, &[extension])
        .set_file_name(file_name)
        .save_file()
        .await
    {
        tokio::fs::write(path.path(), bytes)
            .await
            .context("Failed to write to disk")?;
        info!("Saved successfully to {:?}", path.path());
    } else {
        info!("User cancelled the save dialog.");
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn save_file(
    bytes: &[u8],
    file_name: &str,
    _filter_name: &str,
    _extension: &str,
    mime_type: &str,
) -> anyhow::Result<()> {
    use anyhow::anyhow;
    use wasm_bindgen::JsCast;

    let res = (|| {
        let uint8_array = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::of1(&uint8_array);

        let options = web_sys::BlobPropertyBag::new();
        options.set_type(mime_type);

        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)?;

        let window =
            web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("No window"))?;
        let document = window
            .document()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("No document"))?;

        let a = document
            .create_element("a")?
            .dyn_into::<web_sys::HtmlElement>()?;

        a.set_attribute("href", &url)?;
        a.set_attribute("download", file_name)?;
        a.click();

        web_sys::Url::revoke_object_url(&url)?;
        Ok::<(), wasm_bindgen::JsValue>(())
    })();

    res.map_err(|e| anyhow!("JS error saving file: {:?}", e))
}
