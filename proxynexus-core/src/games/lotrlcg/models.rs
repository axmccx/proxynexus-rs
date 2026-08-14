use serde::{Deserialize, Deserializer};

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
    #[serde(default, deserialize_with = "sideslots")]
    pub sideslots: std::collections::HashMap<String, i64>,
}

fn sideslots<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<std::collections::HashMap<String, i64>, D::Error> {
    Ok(match CodeMap::deserialize(d)? {
        CodeMap::Map(map) => map,
        CodeMap::Empty(_) => Default::default(),
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CodeMap {
    Map(std::collections::HashMap<String, i64>),
    Empty([i64; 0]),
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

#[cfg(test)]
mod tests {
    use super::RingsdbDecklist;

    #[test]
    fn empty_sideboard_arrives_as_a_json_array() {
        let d: RingsdbDecklist =
            serde_json::from_str(r#"{"slots":{"01002":1},"sideslots":[]}"#).unwrap();
        assert!(d.sideslots.is_empty());
    }

    #[test]
    fn populated_sideboard_arrives_as_a_json_object() {
        let d: RingsdbDecklist =
            serde_json::from_str(r#"{"slots":{"01002":1},"sideslots":{"08145":3}}"#).unwrap();
        assert_eq!(d.sideslots.get("08145"), Some(&3));
    }

    #[test]
    fn absent_sideboard_is_allowed() {
        let d: RingsdbDecklist = serde_json::from_str(r#"{"slots":{"01002":1}}"#).unwrap();
        assert!(d.sideslots.is_empty());
    }
}
