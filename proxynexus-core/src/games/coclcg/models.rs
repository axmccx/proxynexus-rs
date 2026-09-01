use serde::Deserialize;

/// A pack (set/expansion) from `coc_packs.json`.
///
/// Example:
/// ```json
/// {
///   "code": "secrets-of-arkham",
///   "name": "Secrets of Arkham",
///   "date_release": "2010-05-27"
/// },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct CocPack {
    pub code: String,
    pub name: String,
    pub date_release: Option<String>,
}

/// A card from `coc_cards.json`, the bulk file covering every card in every
/// pack. Both files are generated from the BoardGameGeek collection
/// spreadsheet by `utils/image_file_renamers/coclcg/build_catalog.py`.
///
/// Example:
/// ```json
/// {
///   "unique_id": "the-necronomicon-al-azif",
///   "name": "The Necronomicon (Al Azif)",
///   "subtitle": "Al Azif",
///   "type": "Support",
///   "faction": "Miskatonic",
///   "back_group": "card",
///   "versions": [
///     { "pack_code": "the-unspeakable-pages", "number": 90, "quantity": 3 }
///   ]
/// },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct CocCard {
    pub unique_id: String,
    pub name: String,
    pub subtitle: Option<String>,
    #[serde(rename = "type")]
    pub card_type: String,
    pub faction: String,
    pub back_group: String,
    pub versions: Vec<CocVersion>,
}

/// One printing of a card: the pack it is in, the number printed on it, and
/// how many copies that pack holds. Promos carry no number.
#[derive(Debug, Clone, Deserialize)]
pub struct CocVersion {
    pub pack_code: String,
    pub number: Option<i64>,
    pub quantity: i64,
}
