use serde::Deserialize;

/// A pack (set/expansion) from `whc_packs.json`.
///
/// Example:
/// ```json
/// {
///   "code": "zogworts-curse",
///   "name": "Zogwort's Curse",
///   "date_release": "2015-02-01"
/// },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct WhcPack {
    pub code: String,
    pub name: String,
    pub date_release: Option<String>,
}

/// A card from `whc_cards.json`, the bulk file covering every card in every
/// pack. Both files are generated from the warhammer_40K_conquest_card_data
/// OCTGN dump by `utils/image_file_renamers/whconquest/build_catalog.py`;
///
/// Example:
/// ```json
/// {
///   "unique_id": "captain-cato-sicarius",
///   "name": "Captain Cato Sicarius",
///   "pack_code": "core-set",
///   "type": "Warlord",
///   "faction": "Space Marine",
///   "card_number": 1,
///   "card_quantity": 1
/// },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct WhcCard {
    pub unique_id: String,
    pub name: String,
    pub pack_code: String,
    #[serde(rename = "type")]
    pub card_type: String,
    pub faction: String,
    pub card_number: Option<i64>,
    pub card_quantity: i64,
}
