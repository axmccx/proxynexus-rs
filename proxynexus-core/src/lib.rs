mod border_generator;
pub mod card_source;
pub mod card_store;
#[cfg(not(target_arch = "wasm32"))]
pub mod catalog;
#[cfg(not(target_arch = "wasm32"))]
pub mod collection_builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod collection_manager;
pub mod db_storage;
#[cfg(not(target_arch = "wasm32"))]
pub mod local_image_provider;
mod models;
pub mod mpc;
pub mod netrunnerdb;
pub mod pdf;
pub mod query;

pub trait ImageProvider: Send + Sync {
    #![allow(async_fn_in_trait)]
    async fn get_image_bytes(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
}
