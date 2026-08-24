#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub game: String,
    pub version: String,
    pub language: String,
    pub generated_date: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BleedPreference {
    Bleed,
    NoBleed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceImage {
    pub key: String,
    pub has_bleed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintingPart {
    pub name: String,
    pub image_key: Option<String>,
    pub bleed_image_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printing {
    pub card_id: String,
    pub card_title: String,
    pub is_official: bool,
    pub variant: Option<String>,
    pub front: PrintingPart,
    pub parts: Vec<PrintingPart>,
    pub collection: String,
    pub side: String,
    pub pack_id: Option<String>,
    pub date_release: Option<String>,
    pub position: Option<i64>,
}

impl PrintingPart {
    pub fn image(&self, preferred: BleedPreference) -> Option<SourceImage> {
        let bleed = || {
            self.bleed_image_key.clone().map(|key| SourceImage {
                key,
                has_bleed: true,
            })
        };
        let no_bleed = || {
            self.image_key.clone().map(|key| SourceImage {
                key,
                has_bleed: false,
            })
        };

        match preferred {
            BleedPreference::Bleed => bleed().or_else(no_bleed),
            BleedPreference::NoBleed => no_bleed().or_else(bleed),
        }
    }
}

impl Printing {
    pub fn variant_key(&self) -> String {
        let display = self
            .pack_id
            .as_deref()
            .or(self.variant.as_deref())
            .unwrap_or("official");
        let position = self.position.map_or(String::new(), |pos| pos.to_string());
        format!("{}:{}:{}", display, position, self.collection)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardRequest {
    pub title: String,
    pub id: String,
    pub printing: Option<String>,
    pub collection: Option<String>,
    pub position: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DecklistEntry {
    pub card_id: String,
    pub pack_id: Option<String>,
    pub quantity: u32,
    pub position: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Decklist {
    pub cards: Vec<DecklistEntry>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedCardRequests {
    pub requests: Vec<CardRequest>,
    pub not_found: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedPrintings {
    pub printings: Vec<Printing>,
    pub available_variants: std::collections::HashMap<String, Vec<Printing>>,
    pub not_found: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printing(pack_id: Option<&str>, variant: Option<&str>, position: Option<i64>) -> Printing {
        Printing {
            card_id: "gandalf_core".into(),
            card_title: "Gandalf".into(),
            is_official: true,
            variant: variant.map(|v| v.to_string()),
            front: PrintingPart {
                name: "front".into(),
                image_key: None,
                bleed_image_key: None,
            },
            parts: Vec::new(),
            collection: "enhanced".into(),
            side: "player".into(),
            pack_id: pack_id.map(|p| p.to_string()),
            date_release: None,
            position,
        }
    }

    #[test]
    fn variant_key_uses_the_pack_when_present() {
        assert_eq!(
            printing(Some("core_set"), None, None).variant_key(),
            "core_set::enhanced"
        );
    }

    #[test]
    fn variant_key_falls_back_to_the_variant_name() {
        assert_eq!(
            printing(None, Some("alt1"), None).variant_key(),
            "alt1::enhanced"
        );
    }

    #[test]
    fn variant_key_falls_back_to_official_with_neither() {
        assert_eq!(
            printing(None, None, None).variant_key(),
            "official::enhanced"
        );
    }

    #[test]
    fn variant_key_distinguishes_two_printings_of_one_card_in_one_pack() {
        let gandalf_4 = printing(Some("two_player_limited_edition_starter"), None, Some(4));
        let gandalf_37 = printing(Some("two_player_limited_edition_starter"), None, Some(37));

        assert_eq!(
            gandalf_4.variant_key(),
            "two_player_limited_edition_starter:4:enhanced"
        );
        assert_eq!(
            gandalf_37.variant_key(),
            "two_player_limited_edition_starter:37:enhanced"
        );
        assert_ne!(gandalf_4.variant_key(), gandalf_37.variant_key());
    }
}
