use serde::Deserialize;

/// A pack (set/expansion) from `swlcg_packs.json`.
///
/// Example:
/// ```json
///  {
///    "code": "core-set",
///    "name": "Core Set",
///    "position": 1
///  },
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct SwlcgPack {
    pub code: String,
    pub name: String,
    pub position: i64,
}

/// A card from `swlcg_card_ids.json`.
///
/// Example:
/// ```json
///  {
///    "unique_id": "0503",
///    "name": "Prophet of the Dark Side",
///    "label": "Prophet of the Dark Side (50.3)",
///    "pack_code": "a-dark-time",
///    "card_type": "Unit",
///    "side": "Dark",
///    "affiliation": "Sith",
///    "affiliation_lock": null,
///    "objective_set_number": 50,
///    "objective_set_sequence": 3,
///    "is_unique": false
///  },
/// ```

#[derive(Debug, Clone, Deserialize)]
pub struct SwlcgCard {
    /// The card's unique code. Matches the `{card_id}` portion of the image
    /// naming convention.
    pub unique_id: String,
    /// The card's name.
    pub name: String,
    /// The card's label. Combination of the card's name and its pack code, to resolve
    /// duplicate names across packs. Example: "Prophet of the Dark Side (50.3)".
    pub label: String,
    /// Matches the `{pack_id}` portion of the image naming convention.
    pub pack_code: String,
    /// Card's type. Options: Affiliation, Enhancement, Event, Fate, Mission, 
    ///  Objective, Unit.
    pub card_type: String,
    /// Card's side. Options: Dark, Light.
    pub side: String,
    /// Card's affiliation. Options: Imperial Navy, Jedi, Neutral,
    ///  Rebel Alliance, Scum and Villainy, Sith, Smugglers and Spies.
    pub affiliation: String,
    /// The objective set number for the card. Sent in the JSON as an integer.
    pub objective_set_number: Option<i64>,
    /// The card's position within the objective set. Sent as an integer.
    pub objective_set_sequence: Option<i64>,
}