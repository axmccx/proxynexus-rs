#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub game: String,
    pub version: String,
    pub language: String,
    pub generated_date: String,
    #[serde(default)]
    pub restricted_back_labels: Vec<String>,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CardSide {
    pub image_key: Option<String>,
    pub bleed_image_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printing {
    pub card_id: String,
    pub card_title: String,
    pub is_official: bool,
    pub variant: Option<String>,
    pub front: CardSide,
    pub backs: Vec<CardSide>,
    pub collection: String,
    pub back_group: String,
    pub pack_id: Option<String>,
    pub date_release: Option<String>,
    pub position: Option<i64>,
}

impl CardSide {
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

    fn cards(&self) -> Vec<PrintedCard<'_>> {
        if self.backs.is_empty() {
            return vec![PrintedCard {
                printing: self,
                front: &self.front,
                back: None,
            }];
        }

        self.backs
            .iter()
            .map(|back| PrintedCard {
                printing: self,
                front: &self.front,
                back: Some(back),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrintedCard<'a> {
    pub printing: &'a Printing,
    pub front: &'a CardSide,
    pub back: Option<&'a CardSide>,
}

/// The cards to print, each with the index of the printing it came from.
pub fn expand_to_cards(printings: &[Printing]) -> Vec<(usize, PrintedCard<'_>)> {
    let mut cards = Vec::new();
    let mut first = 0;

    for copies in printings.chunk_by(|a, b| a == b) {
        // printing has more than one back, provide one set of PrintedCards regardless of how many are requested
        if copies[0].backs.len() > 1 {
            cards.extend(copies[0].cards().into_iter().map(|card| (first, card)));
        } else {
            for (offset, printing) in copies.iter().enumerate() {
                cards.extend(
                    printing
                        .cards()
                        .into_iter()
                        .map(|card| (first + offset, card)),
                );
            }
        }

        first += copies.len();
    }

    cards
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
            front: CardSide {
                image_key: None,
                bleed_image_key: None,
            },
            backs: Vec::new(),
            collection: "enhanced".into(),
            back_group: "player".into(),
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
    }

    fn side(key: &str) -> CardSide {
        CardSide {
            image_key: Some(key.to_string()),
            bleed_image_key: None,
        }
    }

    fn card(card_id: &str, backs: &[&str]) -> Printing {
        let mut printing = printing(Some("core"), None, None);
        printing.card_id = card_id.to_string();
        printing.front = side(&format!("{}_front", card_id));
        printing.backs = backs.iter().map(|key| side(key)).collect();
        printing
    }

    fn back_keys<'a>(cards: &[PrintedCard<'a>]) -> Vec<Option<&'a str>> {
        cards
            .iter()
            .map(|c| c.back.and_then(|b| b.image_key.as_deref()))
            .collect()
    }

    fn printed(printings: &[Printing]) -> Vec<PrintedCard<'_>> {
        expand_to_cards(printings)
            .into_iter()
            .map(|(_, card)| card)
            .collect()
    }

    fn both_scans() -> CardSide {
        CardSide {
            image_key: Some("plain.jpg".into()),
            bleed_image_key: Some("bled.jpg".into()),
        }
    }

    #[test]
    fn the_preferred_scan_is_used_when_both_are_present() {
        let side = both_scans();

        assert_eq!(
            side.image(BleedPreference::Bleed),
            Some(SourceImage {
                key: "bled.jpg".into(),
                has_bleed: true
            })
        );
        assert_eq!(
            side.image(BleedPreference::NoBleed),
            Some(SourceImage {
                key: "plain.jpg".into(),
                has_bleed: false
            })
        );
    }

    #[test]
    fn a_side_with_only_a_bled_scan_falls_back_to_it() {
        // Every lotrlcg image is bled, so this is the path that whole
        // catalogue takes through the PDF layout.
        let mut side = both_scans();
        side.image_key = None;

        assert_eq!(
            side.image(BleedPreference::NoBleed),
            Some(SourceImage {
                key: "bled.jpg".into(),
                has_bleed: true
            })
        );
    }

    #[test]
    fn a_side_with_only_a_plain_scan_falls_back_to_it() {
        let mut side = both_scans();
        side.bleed_image_key = None;

        assert_eq!(
            side.image(BleedPreference::Bleed),
            Some(SourceImage {
                key: "plain.jpg".into(),
                has_bleed: false
            })
        );
    }

    #[test]
    fn a_side_with_no_scan_has_no_image() {
        let side = CardSide::default();

        assert_eq!(side.image(BleedPreference::Bleed), None);
        assert_eq!(side.image(BleedPreference::NoBleed), None);
    }

    #[test]
    fn a_printing_with_no_backs_is_one_single_sided_card() {
        let printing = card("hedge_fund", &[]);
        let cards = printing.cards();

        assert_eq!(cards.len(), 1);
        assert_eq!(
            cards[0].front.image_key.as_deref(),
            Some("hedge_fund_front")
        );
        assert!(cards[0].back.is_none());
    }

    #[test]
    fn a_printing_with_one_back_is_one_double_sided_card() {
        let printing = card("sync", &["sync_back"]);
        let cards = printing.cards();

        assert_eq!(cards.len(), 1);
        assert_eq!(back_keys(&cards), vec![Some("sync_back")]);
    }

    #[test]
    fn several_backs_are_several_cards_sharing_one_front() {
        let printing = card("jinteki", &["b1", "b2", "b3"]);
        let cards = printing.cards();

        assert_eq!(cards.len(), 3);
        assert_eq!(back_keys(&cards), vec![Some("b1"), Some("b2"), Some("b3")]);
        for c in &cards {
            assert_eq!(c.front.image_key.as_deref(), Some("jinteki_front"));
        }
    }

    #[test]
    fn any_number_of_copies_of_a_multi_back_card_prints_one_of_each() {
        for copies in [1, 2, 3, 4, 10] {
            let printings = vec![card("jinteki", &["b1", "b2", "b3"]); copies];

            assert_eq!(
                back_keys(&printed(&printings)),
                vec![Some("b1"), Some("b2"), Some("b3")],
                "copies: {}",
                copies
            );
        }
    }

    #[test]
    fn an_ordinary_card_prints_the_number_of_copies_asked_for() {
        for copies in [1, 2, 3] {
            let printings = vec![card("hedge_fund", &[]); copies];
            let cards = printed(&printings);

            assert_eq!(cards.len(), copies, "copies: {}", copies);
            for c in &cards {
                assert_eq!(c.front.image_key.as_deref(), Some("hedge_fund_front"));
                assert!(c.back.is_none());
            }
        }
    }

    #[test]
    fn a_double_sided_card_is_one_card_printed_as_many_times_as_asked() {
        let printings = vec![card("sync", &["sync_back"]); 3];

        assert_eq!(
            back_keys(&printed(&printings)),
            vec![Some("sync_back"), Some("sync_back"), Some("sync_back")]
        );
    }

    #[test]
    fn two_printings_of_one_card_in_one_pack_are_not_copies_of_each_other() {
        let mut first = card("gandalf", &[]);
        first.position = Some(4);
        let mut second = card("gandalf", &[]);
        second.position = Some(37);

        assert_eq!(printed(&[first, second]).len(), 2);
    }

    #[test]
    fn different_cards_are_never_collapsed() {
        let printings = vec![
            card("jinteki", &["b1", "b2", "b3"]),
            card("hedge_fund", &[]),
            card("jinteki", &["b1", "b2", "b3"]),
        ];

        assert_eq!(printed(&printings).len(), 7);
    }
}
