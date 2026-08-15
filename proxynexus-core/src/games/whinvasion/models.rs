use serde::{Deserialize, Deserializer};

/// A pack (set/expansion) from `whi_packs.json`.
///
/// Example:
/// ```json
/// {
///    "code": "core-set",
///    "name": "Core Set",
///    "position": 1,
///    "total_cards": null,
///    "total_unique": 131,
///    "available": "2009-09-01"
/// },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct WhiPack {
    pub code: String,
    pub name: String,
    pub position: i64,
    pub available: Option<String>,
}

/// A card from `whi_full.json` -- a single bulk file covering every card in
/// every pack (unlike ArkhamDB/MarvelCDB's per-pack REST APIs), fetched once
/// rather than per-pack. There isn't a "double-sided" concept here: unlike
/// AHLCG/Marvel Champions, Warhammer Invasion has no investigator-style
/// flip cards and no hero/alter-ego pairs -- confirmed against the actual
/// collection (1133 cards, 1133 images, zero `~back` files), so each
/// `WhiCard` maps 1:1 to one physical card with one generic back.
///
/// Example:
/// ```json
///   {
///     "name": "'Idden Boy",
///     "race": "Orc",
///     "type": "Unit",
///     "pack_code": "days-of-blood",
///     "card_number": "13",
///     "card_quantity": "3",
///     "unique_id": "10013"
///   },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct WhiCard {
    /// The card's unique code. Matches the `{card_id}` portion of the image
    /// naming convention.
    pub unique_id: String,
    /// The card's name.
    pub name: String,
    /// Matches the `{pack_id}` portion of the image naming convention.
    pub pack_code: String,
    /// Card type. Options: Draft, Fulcrum, Legend, Quest, Support, Tactic, Unit.
    #[serde(rename = "type")]
    pub card_type: String,
    /// Player faction (Chaos, Dark Elf, Dwarf, Empire, High Elf, Orc) or
    /// "Neutral".
    pub race: String,
    /// How many copies of this card are in a playset. Sent by the source
    /// JSON as either absent/null or a numeric *string* (e.g. `"3"`), never
    /// a bare number -- `deserialize_option_i64` handles both.
    #[serde(default, deserialize_with = "deserialize_option_i64")]
    pub card_quantity: Option<i64>,
    /// The card's position within its pack. Also sent as a numeric string.
    #[serde(default, deserialize_with = "deserialize_option_i64")]
    pub card_number: Option<i64>,
}

/// The source JSON sends numeric fields like `card_quantity`/`card_number`
/// as strings (or omits/nulls them), never as bare JSON numbers -- confirmed
/// against the live `whi_full.json` (1133/1133 cards parse cleanly as
/// strings; zero bare-number or non-numeric values found).
fn deserialize_option_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;
    match raw {
        None => Ok(None),
        Some(s) => s.parse::<i64>().map(Some).map_err(serde::de::Error::custom),
    }
}

// There isn't currently a decklist source for Warhammer Invasion, so no
// WhiDecklist type exists yet -- see api.rs's fetch_decklist_from_url.
