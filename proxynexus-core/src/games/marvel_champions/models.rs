use serde::Deserialize;

/// A pack from MarvelCDB's `/api/public/packs/` endpoint.
///
/// Example:
/// ```json
/// {
///   "code": "core",
///   "name": "Core Set",
///   "position": 1,
///   "available": "2019-11-01"
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct McdbPack {
    pub code: String,
    pub name: String,
    /// MarvelCDB's release-date field is named "available"
    pub available: Option<String>,
}

/// A card from MarvelCDB's `/api/public/cards/` endpoint.
///
/// Example:
/// ```json
/// {
///   "code": "01094",
///   "name": "Rhino",
///   "pack_code": "core",
///   "position": 94,
///   "type_code": "villain",
///   "stage": "I",
///   "hidden": false,
///   "quantity": 1
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct McdbCard {
    pub code: String,
    pub name: String,
    pub pack_code: String,
    pub position: i64,
    pub type_code: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub linked_to_code: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub duplicate_of_code: Option<String>,
    pub quantity: Option<i64>,
}
