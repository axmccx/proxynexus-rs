use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbResponse<T> {
    pub data: Vec<T>,
    pub links: Option<NrdbLinks>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbLinks {
    pub next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCard {
    pub id: String,
    pub attributes: NrdbCardAttributes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCardAttributes {
    pub title: String,
    pub stripped_title: String,
    pub side_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCardSet {
    pub id: String,
    pub attributes: NrdbCardSetAttributes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbCardSetAttributes {
    pub name: String,
    pub date_release: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbPrinting {
    pub id: String,
    pub attributes: NrdbPrintingAttributes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NrdbPrintingAttributes {
    pub card_id: String,
    pub card_set_id: String,
    pub quantity: i64,
}
