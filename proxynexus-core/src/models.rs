#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub language: String,
    pub generated_date: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    pub code: String,
    pub title: String,
    pub pack_code: String,
    pub side_code: String,
    pub quantity: u32,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct Pack {
    pub code: String,
    pub name: String,
    pub date_release: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printing {
    pub card_code: String,
    pub card_title: String,
    pub variant: String,
    pub image_key: String,
    pub collection: String,
    pub side: String,
    pub pack_code: String,
}

#[derive(Debug, Clone)]
pub struct CardRequest {
    pub title: String,
    pub code: String,
    pub variant: Option<String>,
    pub collection: Option<String>,
    pub pack_code: Option<String>,
}
