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
}

#[derive(Debug, Clone, Deserialize)]
pub struct AhdbDecklist {
    pub slots: std::collections::HashMap<String, i64>,
}

#[cfg(test)]
mod tests {
    use super::AhdbCard;

    fn parse(json: &str) -> AhdbCard {
        serde_json::from_str(json).expect("card should deserialize")
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
