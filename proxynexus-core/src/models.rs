#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub game: String,
    pub version: String,
    pub language: String,
    pub generated_date: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintingPart {
    pub name: String,
    pub image_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printing {
    pub card_id: String,
    pub card_title: String,
    pub is_official: bool,
    pub variant: Option<String>,
    pub image_key: String,
    pub parts: Vec<PrintingPart>,
    pub collection: String,
    pub side: String,
    pub pack_id: Option<String>,
    pub date_release: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CardRequest {
    pub title: String,
    pub id: String,
    pub variant: Option<String>,
    pub collection: Option<String>,
    pub pack_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DecklistEntry {
    pub card_id: String,
    pub pack_id: Option<String>,
    pub quantity: u32,
}

#[derive(Debug, Clone)]
pub struct Decklist {
    pub cards: Vec<DecklistEntry>,
}
