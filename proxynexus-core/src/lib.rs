pub mod card_source;
pub mod card_store;
#[cfg(not(target_arch = "wasm32"))]
pub mod catalog;
#[cfg(not(target_arch = "wasm32"))]
pub mod collection_builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod collection_manager;
pub mod db_storage;
pub mod error;
pub mod games;
pub mod image_provider;
pub mod models;
pub mod mpc;
pub mod pdf;
pub mod print_prep;
pub mod query;

#[cfg(feature = "upscaling")]
pub mod upscaler;

pub async fn upscale_image(bytes: &[u8]) -> error::Result<Vec<u8>> {
    #[cfg(feature = "upscaling")]
    {
        upscaler::upscale_image(bytes).await
    }

    #[cfg(not(feature = "upscaling"))]
    {
        let _ = bytes;
        Err(error::ProxyNexusError::Internal(
            "AI upscaling is not enabled in this build. Rebuild with '--features upscaling' to enable it.".to_string()
        ))
    }
}

pub async fn probe_gpu() -> bool {
    #[cfg(all(feature = "upscaling", target_arch = "wasm32"))]
    {
        upscaler::probe_webgpu().await
    }
    #[cfg(all(feature = "upscaling", not(target_arch = "wasm32")))]
    {
        true
    }
    #[cfg(not(feature = "upscaling"))]
    {
        false
    }
}
