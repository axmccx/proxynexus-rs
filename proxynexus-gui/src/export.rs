use dioxus::prelude::*;
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::pdf::PageSize;
use tracing::{error, info};

pub async fn run_pdf_export(mut db_signal: Signal<DbStorage>, text: String, page_size: PageSize) {
    info!("Starting PDF generation with page size: {:?}", page_size);

    let source = proxynexus_core::card_source::Cardlist(text);
    let mut db = db_signal.write();

    #[cfg(not(target_arch = "wasm32"))]
    let provider = {
        let home = dirs::home_dir().expect("Could not find home directory");
        let collections_path = home.join(".proxynexus").join("collections");
        proxynexus_core::image_provider::LocalImageProvider::new(collections_path)
    };

    #[cfg(target_arch = "wasm32")]
    let provider = proxynexus_core::image_provider::RemoteImageProvider;

    match proxynexus_core::pdf::generate_pdf(&mut db, &source, &provider, page_size).await {
        Ok(pdf_bytes) => {
            info!(
                "Successfully generated PDF. Size: {} bytes",
                pdf_bytes.len()
            );

            if let Err(e) = save_pdf(&pdf_bytes).await {
                error!("Failed to save PDF: {:?}", e);
            }
        }
        Err(e) => {
            error!("Failed to generate PDF: {}", e);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn save_pdf(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = rfd::AsyncFileDialog::new()
        .add_filter("PDF Document", &["pdf"])
        .set_file_name("proxynexus_export.pdf")
        .save_file()
        .await
    {
        tokio::fs::write(path.path(), bytes).await?;
        info!("Saved PDF successfully to {:?}", path.path());
    } else {
        info!("User cancelled the save dialog.");
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn save_pdf(bytes: &[u8]) -> Result<(), wasm_bindgen::JsValue> {
    // Using native browser APIs to create a Blob directly from WASM memory.
    // This avoids JSON serialization overhead of Dioxus eval, which causes overflow errors on large PDFs
    use wasm_bindgen::JsCast;

    let uint8_array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&uint8_array);

    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/pdf");

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
    a.set_attribute("download", "proxynexus_result.pdf")?;
    a.click();

    web_sys::Url::revoke_object_url(&url)?;

    Ok(())
}
