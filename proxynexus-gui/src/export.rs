#![allow(clippy::await_holding_invalid_type)]

use crate::analytics;
use crate::components::export_controls::ExportConfig;
use crate::components::source_selector::ActiveSource;
use anyhow::Context;
use dioxus::prelude::*;
use proxynexus_core::card_source::{CardSource, Cardlist, NrdbUrl, SetName};
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::mpc::generate_mpc_zip;
use proxynexus_core::pdf::PdfOptions;
use proxynexus_core::pdf::generate_pdf;
use proxynexus_core::query::apply_variant_overrides;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{error, info};
use web_time::Instant;

struct ExportMeta {
    format: &'static str,
    options: Option<PdfOptions>,
    filename: &'static str,
    filter: &'static str,
    ext: &'static str,
    mime: &'static str,
}

pub async fn run_export(
    mut db_signal: Signal<DbStorage>,
    active_source: ActiveSource,
    config: ExportConfig,
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

    let meta = match config.clone() {
        ExportConfig::Pdf(options) => ExportMeta {
            format: "pdf",
            options: Some(options),
            filename: "proxynexus_export.pdf",
            filter: "PDF Document",
            ext: "pdf",
            mime: "application/pdf",
        },
        ExportConfig::Mpc => ExportMeta {
            format: "mpc",
            options: None,
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
        ActiveSource::NrdbUrl(url) => (url.clone(), "NrdbUrl"),
    };

    let resolved_printings = async {
        let mut db = db_signal.write();
        let mut store = proxynexus_core::card_store::CardStore::new(&mut db)
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
            ActiveSource::NrdbUrl(url) => NrdbUrl(url)
                .to_card_requests(&mut store)
                .await
                .context("Failed to fetch deck from NetrunnerDB")?,
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
                format!(
                    "{} [{}:{}:{}]",
                    p.card_title, p.variant, p.collection, p.pack_code
                )
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
        (Ok(printings), Ok(image_provider)) => match config {
            ExportConfig::Pdf(options) => {
                generate_pdf(printings, &image_provider, options, progress_callback)
                    .await
                    .context("PDF generation failed")
            }
            ExportConfig::Mpc => generate_mpc_zip(printings, &image_provider, progress_callback)
                .await
                .context("MPC ZIP generation failed"),
        },
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e),
    };

    let mut success = false;
    let mut error_message = None;

    match &result {
        Ok(bytes) => {
            success = true;
            info!(
                "Successfully generated {}. Size: {} bytes",
                meta.format,
                bytes.len()
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
