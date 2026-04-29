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
pub struct NrdbResponse<T> {
    pub data: Vec<T>,
    pub links: Option<NrdbLinks>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct NrdbLinks {
    pub next: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCard {
    pub id: String,
    pub attributes: NrdbCardAttributes,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCardAttributes {
    pub title: String,
    pub stripped_title: String,
    pub side_id: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCardSet {
    pub id: String,
    pub attributes: NrdbCardSetAttributes,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCardSetAttributes {
    pub name: String,
    pub date_release: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct NrdbPrinting {
    pub id: String,
    pub attributes: NrdbPrintingAttributes,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
pub struct NrdbPrintingAttributes {
    pub card_id: String,
    pub card_set_id: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintingPart {
    pub name: String,
    pub image_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printing {
    pub card_code: String,
    pub card_title: String,
    pub variant: String,
    pub image_key: String,
    pub parts: Vec<PrintingPart>,
    pub collection: String,
    pub side: String,
    pub pack_code: String,
    pub date_release: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CardRequest {
    pub title: String,
    pub code: String,
    pub variant: Option<String>,
    pub collection: Option<String>,
    pub pack_code: Option<String>,
}
