use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub language: String,
    pub generated_date: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    pub code: String,
    pub title: String,
    pub pack_code: String,
    pub side_code: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pack {
    pub code: String,
    pub name: String,
    pub date_release: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Printing {
    pub card_code: String,
    pub card_title: String,
    pub variant: String,
    pub file_path: PathBuf,
    pub collection: String,
    pub side: String,
}

#[derive(Debug, Clone)]
pub struct CardRequest {
    pub code: String,
    pub variant: Option<String>,
    pub collection: Option<String>,
}
