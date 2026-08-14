use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct HobCardFront {
    pub image_path: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct HobCard {
    pub title: String,
    pub slug: String,
    pub card_set: String,
    pub number: i64,
    pub quantity: Option<i64>,
    pub front: Option<HobCardFront>,
    pub card_type: String,
}

#[derive(Deserialize, Debug)]
pub struct RingsdbDecklist {
    pub slots: std::collections::HashMap<String, i64>,
    #[serde(default)]
    pub sideslots: std::collections::HashMap<String, i64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RingsdbCard {
    pub code: String,
    pub name: String,
    pub pack_code: String,
    pub pack_name: String,
    pub type_code: Option<String>,
    pub position: Option<u32>,
    pub quantity: Option<u32>,
    pub is_official: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RingsdbPack {
    pub name: String,
    pub available: String,
}
