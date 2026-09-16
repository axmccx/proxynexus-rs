use serde::Deserialize;

/// A pack (set/expansion) from ArkhamDB's `/api/public/packs/` endpoint.
///
/// Example:
/// ```json
/// {
///   "code": "core",
///   "name": "Core Set",
///   "position": 1,
///   "available": "2016-11-10",
///   "known": 185,
///   "total": 184
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct AhdbPack {
    pub code: String,
    pub name: String,
    pub position: i64,
    /// ArkhamDB's release-date field is named "available", not "date_release".
    pub available: Option<String>,
    /// "campaign" or "player" on a repackaged expansion, absent on every other
    /// pack.
    #[serde(default)]
    pub reprint_type: Option<String>,
    /// The packs a repackaged expansion reprints. No card is filed under the
    /// repackage itself: each keeps the `pack_code` of its first printing.
    #[serde(default)]
    pub reprint_packs: Vec<String>,
}

/// A card from ArkhamDB's `/api/public/cards/{pack_code}` endpoint. Covers
/// investigator, player, and encounter cards.
///
/// Player card example:
/// ```json
/// {
///   "code": "01001",
///   "name": "Roland Banks",
///   "pack_code": "core",
///   "position": 1,
///   "type_code": "investigator",
///   "faction_code": "guardian",
///   "double_sided": true,
///   "quantity": 1
/// }
/// ```

#[derive(Debug, Clone, Deserialize)]
pub struct AhdbCard {
    pub code: String,
    pub name: String,
    pub pack_code: String,
    pub position: i64,
    pub type_code: String,
    pub faction_code: String,
    #[serde(default)]
    pub quantity: Option<i64>,
    #[serde(default)]
    pub subtype_code: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub xp: Option<i64>,
    #[serde(default)]
    pub subname: Option<String>,
    #[serde(default)]
    pub duplicated_by: Vec<String>,
    /// The encounter set the card belongs to. Present on encounter cards and
    /// absent on player cards, which is what separates a repackaged campaign
    /// expansion from its investigator expansion.
    #[serde(default)]
    pub encounter_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AhdbDecklist {
    pub slots: std::collections::HashMap<String, i64>,
}

#[cfg(test)]
mod tests {
    use super::{AhdbCard, AhdbPack};

    fn parse(json: &str) -> AhdbCard {
        serde_json::from_str(json).expect("card should deserialize")
    }

    fn parse_pack(json: &str) -> AhdbPack {
        serde_json::from_str(json).expect("pack should deserialize")
    }

    #[test]
    fn a_card_carrying_no_hidden_flag_is_not_hidden() {
        // Every ordinary card omits the field, so the default is what almost
        // every record relies on.
        let card = parse(
            r#"{"code":"01001","name":"Roland Banks","pack_code":"core","position":1,
                "type_code":"investigator","faction_code":"guardian"}"#,
        );
        assert!(!card.hidden);
        assert_eq!(card.quantity, None);
        assert_eq!(card.subtype_code, None);
        assert_eq!(card.xp, None);
        assert_eq!(card.subname, None);
        assert!(card.duplicated_by.is_empty());
    }

    #[test]
    fn an_upgrade_carries_its_level() {
        // `01039` Deduction is level 0; `02150` is the level 2 of the same
        // name, and only `xp` says so.
        let base = parse(
            r#"{"code":"01039","name":"Deduction","pack_code":"core","position":39,
                "type_code":"skill","faction_code":"seeker","xp":0}"#,
        );
        assert_eq!(base.xp, Some(0));

        let upgrade = parse(
            r#"{"code":"02150","name":"Deduction","pack_code":"tece","position":150,
                "type_code":"skill","faction_code":"seeker","xp":2}"#,
        );
        assert_eq!(upgrade.xp, Some(2));
    }

    #[test]
    fn a_reprint_is_listed_on_the_card_it_reprints() {
        // `60227` Seeking Answers (2) and `01685` are one card printed twice,
        // with the text reworded between the two.
        let card = parse(
            r#"{"code":"60227","name":"Seeking Answers","pack_code":"har","position":27,
                "type_code":"event","faction_code":"seeker","xp":2,
                "duplicated_by":["01685"]}"#,
        );
        assert_eq!(card.duplicated_by, ["01685"]);
    }

    #[test]
    fn a_subtitle_is_read_where_the_card_has_one() {
        // `03298` Abbey Tower, one of two locations of that name in the pack.
        let card = parse(
            r#"{"code":"03298","name":"Abbey Tower","subname":"The Path is Open",
                "pack_code":"bsr","position":298,"type_code":"location",
                "faction_code":"mythos"}"#,
        );
        assert_eq!(card.subname.as_deref(), Some("The Path is Open"));
    }

    #[test]
    fn an_encounter_card_names_the_set_it_belongs_to() {
        // `03060` Royal Emissary is in the campaign half of the Carcosa
        // repackage; `03042` Astral Travel, carrying no encounter set, is in
        // the investigator half.
        let encounter = parse(
            r#"{"code":"03060","name":"Royal Emissary","pack_code":"ptc","position":60,
                "type_code":"enemy","faction_code":"mythos","encounter_code":"the_last_king"}"#,
        );
        assert_eq!(encounter.encounter_code.as_deref(), Some("the_last_king"));

        let player = parse(
            r#"{"code":"03042","name":"Astral Travel","pack_code":"ptc","position":42,
                "type_code":"event","faction_code":"mystic"}"#,
        );
        assert_eq!(player.encounter_code, None);
    }

    #[test]
    fn a_repackaged_expansion_names_the_packs_it_reprints() {
        let pack = parse_pack(
            r#"{"code":"ptcc","name":"The Path to Carcosa Campaign Expansion","position":2,
                "available":"2024-01-25","reprint_type":"campaign",
                "reprint_packs":["ptc","eotp","tuo","apot","tpm","bsr","dca"]}"#,
        );
        assert_eq!(pack.reprint_type.as_deref(), Some("campaign"));
        assert_eq!(pack.reprint_packs.len(), 7);
    }

    #[test]
    fn an_ordinary_pack_reprints_nothing() {
        let pack = parse_pack(
            r#"{"code":"ptc","name":"The Path to Carcosa","position":1,
                "available":"2017-02-23"}"#,
        );
        assert_eq!(pack.reprint_type, None);
        assert!(pack.reprint_packs.is_empty());
    }

    #[test]
    fn the_half_arkhamdb_hides_is_read_as_hidden() {
        // `03325` Shores of Hali, the face behind `03325b`.
        let card = parse(
            r#"{"code":"03325","name":"Shores of Hali","pack_code":"dca","position":325,
                "type_code":"location","faction_code":"mythos","hidden":true}"#,
        );
        assert!(card.hidden);
    }
}
