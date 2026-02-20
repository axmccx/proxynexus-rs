use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub language: String,
    pub generated_date: String,
}

#[derive(Debug, Clone)]
pub struct CardMetadata {
    pub code: String,
    pub title: String,
    pub set_code: String,
    pub set_name: String,
    pub release_date: Option<String>,
    pub side: String,
    pub quantity: u32,
}

#[derive(Debug, Clone)]
pub struct CollectionPrinting {
    pub card_code: String,
    pub variant: String,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct Printing {
    pub card_code: String,
    pub variant: String,
    pub file_path: String,
    pub collection: String,
    pub side: String,
}

#[derive(Debug, Clone)]
pub struct CardRequest {
    pub code: String,
    pub variant: Option<String>,
    pub collection: Option<String>,
}
