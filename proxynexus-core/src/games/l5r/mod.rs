pub mod api;
pub mod models;
pub mod schema;

#[cfg(not(target_arch = "wasm32"))]
pub mod catalog;
