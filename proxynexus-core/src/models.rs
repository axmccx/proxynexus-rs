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
    pub bleed_image_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printing {
    pub card_id: String,
    pub card_title: String,
    pub is_official: bool,
    pub variant: Option<String>,
    pub image_key: String,
    pub bleed_image_key: Option<String>,
    pub parts: Vec<PrintingPart>,
    pub collection: String,
    pub side: String,
    pub pack_id: Option<String>,
    pub date_release: Option<String>,
}

impl PrintingPart {
    pub fn mpc_image(&self) -> (String, bool) {
        self.bleed_image_key
            .clone()
            .map(|k| (k, true))
            .unwrap_or_else(|| (self.image_key.clone(), false))
    }

    pub fn pdf_image(&self) -> (String, bool) {
        if !self.image_key.is_empty() {
            (self.image_key.clone(), false)
        } else if let Some(bleed) = &self.bleed_image_key {
            (bleed.clone(), true)
        } else {
            (String::new(), false)
        }
    }
}

impl Printing {
    pub fn mpc_image(&self) -> (String, bool) {
        self.bleed_image_key
            .clone()
            .map(|k| (k, true))
            .unwrap_or_else(|| (self.image_key.clone(), false))
    }

    pub fn pdf_image(&self) -> (String, bool) {
        if !self.image_key.is_empty() {
            (self.image_key.clone(), false)
        } else if let Some(bleed) = &self.bleed_image_key {
            (bleed.clone(), true)
        } else {
            (String::new(), false)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardRequest {
    pub title: String,
    pub id: String,
    pub printing: Option<String>,
    pub collection: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DecklistEntry {
    pub card_id: String,
    pub pack_id: Option<String>,
    pub quantity: u32,
    pub position: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Decklist {
    pub cards: Vec<DecklistEntry>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedCardRequests {
    pub requests: Vec<CardRequest>,
    pub not_found: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedPrintings {
    pub printings: Vec<Printing>,
    pub available_variants: std::collections::HashMap<String, Vec<Printing>>,
    pub not_found: Vec<String>,
}
