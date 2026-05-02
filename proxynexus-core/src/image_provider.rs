use crate::error::{ProxyNexusError, Result};

pub trait ImageProvider: Send + Sync {
    #![allow(async_fn_in_trait)]
    async fn get_image_bytes(&self, key: &str) -> Result<Vec<u8>>;
}

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
pub struct LocalImageProvider {
    base_path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl LocalImageProvider {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ImageProvider for LocalImageProvider {
    async fn get_image_bytes(&self, key: &str) -> Result<Vec<u8>> {
        let full_path = self.base_path.join(key);
        let bytes = std::fs::read(&full_path).map_err(|e| {
            ProxyNexusError::Internal(format!(
                "Failed to read image with key {:?} at {:?}: {}",
                key, full_path, e
            ))
        })?;
        Ok(bytes)
    }
}

pub struct RemoteImageProvider;

impl ImageProvider for RemoteImageProvider {
    async fn get_image_bytes(&self, key: &str) -> Result<Vec<u8>> {
        let url = format!("https://test-collections.proxynexus.net/{}", key);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let response = reqwest::get(&url).await?;
            if !response.status().is_success() {
                return Err(ProxyNexusError::Internal(format!(
                    "Failed to fetch image: HTTP {}",
                    response.status()
                )));
            }
            let bytes = response.bytes().await?;
            Ok(bytes.to_vec())
        }

        #[cfg(target_arch = "wasm32")]
        {
            use gloo_net::http::Request;
            let response = Request::get(&url).send().await?;

            if !response.ok() {
                return Err(ProxyNexusError::Internal(format!(
                    "Failed to fetch image: HTTP {}",
                    response.status()
                )));
            }

            let bytes = response.binary().await?;

            Ok(bytes)
        }
    }
}
