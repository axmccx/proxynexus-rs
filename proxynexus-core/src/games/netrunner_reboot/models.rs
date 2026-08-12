use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct RebootResponse<T> {
    pub data: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RebootCard {
    pub code: String,
    pub title: String,
    pub pack_code: String,
    pub side_code: String,
    pub quantity: i64,
    pub position: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RebootPack {
    pub code: String,
    pub name: String,
    pub date_release: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RebootDeck {
    pub cards: HashMap<String, u32>,
}
