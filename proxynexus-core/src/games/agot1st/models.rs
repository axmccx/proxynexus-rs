use serde::Deserialize;

/// A pack (set/expansion) from `agot1st_packs.json`.
///
/// Example:
/// ```json
///   {
///     "code": "kings-of-the-sea",
///     "name": "Kings of the Sea",
///     "position": 2,
///     "total_cards": 158,
///     "total_unique": 54,
///     "available": "2009-07-24"
///   },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct Agot1stPack {
    pub code: String,
    pub name: String,
    pub position: i64,
    pub available: Option<String>,
}

/// A card from `agot1st_card_ids.json`.
///
/// Example:
/// ```json
///  {
///    "id": "dotn_46",
///    "unique_id": "11046",
///    "cycle": "dotn",
///    "cycle_number": 11,
///    "card_number": 46,
///    "set": "A Sword in the Darkness",
///    "name": "The Iron Throne",
///    "pack_code": "a-sword-in-the-darkness",
///    "card_type": "Location",
///    "card_quantity": 1,
///    "house": "Baratheon",
///    "label": "The Iron Throne (a-sword-in-the-darkness)"
///  },
/// ```

#[derive(Debug, Clone, Deserialize)]
pub struct Agot1stCard {
    /// The card's unique code. Matches the `{card_id}` portion of the image
    /// naming convention.
    pub unique_id: String,
    /// The card's name.
    pub name: String,
    /// The card's label. Combination of the card's name and its pack code, to resolve
    /// duplicate names across packs. Example: "The Iron Throne (a-sword-in-the-darkness)".
    pub label: String,
    /// Matches the `{pack_id}` portion of the image naming convention.
    pub pack_code: String,
    /// Card type. Options: Agenda, Attachment, Character, Event, House, Location, Plot.
    pub card_type: String,
    /// Card's house. Options: Baratheon, Greyjoy, Lannister, Martell,
    ///  Neutral, Stark, Targaryen. Can also be a comma separated list
    ///  of multiple houses (e.g. "Stark,Targaryen").
    pub house: String,
    /// How many copies of this card are in a playset. Sent by the source
    /// JSON as an integer.
    pub card_quantity: Option<i64>,
    /// The card's position within its pack. Sent as an integer.
    pub card_number: Option<i64>,
}
