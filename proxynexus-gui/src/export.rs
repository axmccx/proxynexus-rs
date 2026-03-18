use crate::analytics;
use crate::components::export_controls::ExportConfig;
use crate::components::source_selector::ActiveSource;
use dioxus::prelude::*;
use proxynexus_core::card_source::{CardSource, Cardlist, NrdbUrl, SetName};
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::mpc::generate_mpc_zip;
use proxynexus_core::pdf::generate_pdf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{error, info};
use web_time::Instant;

struct ExportMeta {
    format: &'static str,
    page_size: String,
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

    #[cfg(not(target_arch = "wasm32"))]
    let provider = {
        let home = dirs::home_dir().expect("Could not find home directory");
        let collections_path = home.join(".proxynexus").join("collections");
        proxynexus_core::image_provider::LocalImageProvider::new(collections_path)
    };

    #[cfg(target_arch = "wasm32")]
    let provider = proxynexus_core::image_provider::RemoteImageProvider;

    let meta = match config {
        ExportConfig::Pdf(page_size) => ExportMeta {
            format: "pdf",
            page_size: format!("{:?}", page_size),
            filename: "proxynexus_export.pdf",
            filter: "PDF Document",
            ext: "pdf",
            mime: "application/pdf",
        },
        ExportConfig::Mpc => ExportMeta {
            format: "mpc",
            page_size: "N/A".to_string(),
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

    let (source_text, source_type, resolved_printings) = {
        let mut db = db_signal.write();
        let mut store =
            proxynexus_core::card_store::CardStore::new(&mut db).expect("Failed to create store");

        match active_source {
            ActiveSource::Cardlist(text) => {
                let source_text = text.clone();
                let source = Cardlist(text);
                let res = async {
                    let reqs = source.to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)
                }
                .await;
                (source_text, "Cardlist", res)
            }
            ActiveSource::SetName(name) => {
                let source_text = name.clone();
                let source = SetName(name);
                let res = async {
                    let reqs = source.to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)
                }
                .await;
                (source_text, "SetName", res)
            }
            ActiveSource::NrdbUrl(url) => {
                let source_text = url.clone();
                let source = NrdbUrl(url);
                let res = async {
                    let reqs = source.to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)
                }
                .await;
                (source_text, "NrdbUrl", res)
            }
        }
    };

    let result = match resolved_printings {
        Ok(printings) => match config {
            ExportConfig::Pdf(page_size) => {
                generate_pdf(printings, &provider, page_size, progress_callback).await
            }
            ExportConfig::Mpc => generate_mpc_zip(printings, &provider, progress_callback).await,
        },
        Err(e) => Err(e),
    };

    let mut success = false;
    let mut error_message = None;

    match result {
        Ok(bytes) => {
            info!(
                "Successfully generated {}. Size: {} bytes",
                meta.format,
                bytes.len()
            );

            if let Err(e) = save_file(&bytes, meta.filename, meta.filter, meta.ext, meta.mime).await
            {
                let msg = format!("Failed to save {}: {:?}", meta.format, e);
                error!("{}", msg);
                error_message = Some(msg);
            } else {
                success = true;
            }
        }
        Err(e) => {
            let msg = format!("Failed to generate {}: {}", meta.format, e);
            error!("{}", msg);
            error_message = Some(msg);
        }
    }

    if let Some(task) = update_task.take() {
        task.cancel();
    }

    analytics::send_report(analytics::GenerationReport {
        format: meta.format.to_string(),
        page_size: meta.page_size,
        runtime_ms: start_time.elapsed().as_millis(),
        success,
        source_type,
        source_text,
        error_message,
    });

    progress_signal.set(None);
}

#[cfg(not(target_arch = "wasm32"))]
async fn save_file(
    bytes: &[u8],
    file_name: &str,
    filter_name: &str,
    extension: &str,
    _mime_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = rfd::AsyncFileDialog::new()
        .add_filter(filter_name, &[extension])
        .set_file_name(file_name)
        .save_file()
        .await
    {
        tokio::fs::write(path.path(), bytes).await?;
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
) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    let uint8_array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&uint8_array);

    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime_type);

    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;

    let window = web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("No window"))?;
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

    Ok(())
}
