use crate::card_source::DecklistProvider;
#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::GameAdapterInfo;
use crate::games::ahlcg::api::fetch_decklist_from_arkhamdb;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::ahlcg::api::{fetch_all_cards, fetch_packs};
use crate::models::Decklist;
use async_trait::async_trait;

pub struct AhlcgAdapter {}

impl Default for AhlcgAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AhlcgAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for AhlcgAdapter {
    fn game_id(&self) -> &'static str {
        "ahlcg"
    }

    fn game_name(&self) -> &'static str {
        "Arkham Horror (Chapter 1)"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["ahlcg"]
    }
}

// Which generic card back a card needs, classified by `type_code` rather
// than `faction_code`/side
#[cfg(not(target_arch = "wasm32"))]
const PLAYER_TYPES: &[&str] = &["investigator", "asset", "event", "skill"];
#[cfg(not(target_arch = "wasm32"))]
const ENCOUNTER_TYPES: &[&str] = &[
    "enemy",
    "enemy_location",
    "treachery",
    "agenda",
    "act",
    "location",
    "scenario",
    "story",
    "key",
];

#[cfg(not(target_arch = "wasm32"))]
fn back_group_for(type_code: &str, subtype_code: Option<&str>) -> Option<String> {
    if matches!(subtype_code, Some("weakness") | Some("basicweakness")) {
        return Some("player".to_string());
    }
    if PLAYER_TYPES.contains(&type_code) {
        Some("player".to_string())
    } else if ENCOUNTER_TYPES.contains(&type_code) {
        Some("encounter".to_string())
    } else {
        None
    }
}

/// The name ArkhamDB prints on the card. `name` alone is the base name, shared
/// by every level of an upgradeable card, so the level has to be spelled out
/// for the title to name one card.
#[cfg(not(target_arch = "wasm32"))]
fn display_title(name: &str, xp: Option<i64>) -> String {
    match xp {
        Some(xp) if xp > 0 => format!("{} ({})", name, xp),
        _ => name.to_string(),
    }
}

/// ArkhamDB keeps both sides of a double-sided card under one `code` -- both
/// the ordinary flip case and the case where the back is a mechanically distinct card
#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(
    ahdb_cards: Vec<crate::games::ahlcg::models::AhdbCard>,
) -> (Vec<Card>, Vec<CardVersion>) {
    let mut cards = Vec::with_capacity(ahdb_cards.len());
    let mut card_versions = Vec::with_capacity(ahdb_cards.len());

    for card in ahdb_cards {
        let title = display_title(&card.name, card.xp);

        cards.push(Card {
            id: card.code.clone(),
            title_normalized: normalize_title(&title),
            title,
            back_group: back_group_for(&card.type_code, card.subtype_code.as_deref()),
        });

        card_versions.push(CardVersion {
            card_id: card.code,
            pack_id: card.pack_code,
            quantity: card.quantity.unwrap_or(1),
            position: Some(card.position),
            api_id: None,
        });
    }

    (cards, card_versions)
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for AhlcgAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let (ahdb_packs, ahdb_cards) = (fetch_packs().await?, fetch_all_cards().await?);

        let packs: Vec<Pack> = ahdb_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: pack.available,
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(ahdb_cards);

        Ok(Catalog {
            game_id: self.game_id().to_string(),
            display_name: self.game_name().to_string(),
            packs,
            cards,
            card_versions,
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DecklistProvider for AhlcgAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        fetch_decklist_from_arkhamdb(url).await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::games::ahlcg::models::AhdbCard;

    fn card(code: &str, name: &str, type_code: &str, subtype_code: Option<&str>) -> AhdbCard {
        AhdbCard {
            code: code.to_string(),
            name: name.to_string(),
            pack_code: "core".to_string(),
            position: 1,
            type_code: type_code.to_string(),
            faction_code: "neutral".to_string(),
            quantity: Some(1),
            subtype_code: subtype_code.map(|s| s.to_string()),
            hidden: false,
            xp: None,
        }
    }

    fn player_card(code: &str, name: &str, pack_code: &str, position: i64, xp: i64) -> AhdbCard {
        AhdbCard {
            pack_code: pack_code.to_string(),
            position,
            xp: Some(xp),
            ..card(code, name, "skill", None)
        }
    }

    #[test]
    fn player_and_encounter_cards_get_the_correct_back_group() {
        let raw = vec![
            card("01006", "Roland Banks", "investigator", None),
            card("01121", "Ghoul Priest", "enemy", None),
        ];
        let (cards, versions) = build_cards_and_versions(raw);

        assert_eq!(cards.len(), 2);
        assert_eq!(versions.len(), 2);
        assert_eq!(cards[0].back_group.as_deref(), Some("player"));
        assert_eq!(cards[1].back_group.as_deref(), Some("encounter"));
    }

    #[test]
    fn a_weakness_carries_the_player_back_even_with_an_encounter_type_code() {
        // Mob Goons (08003) is type_code "enemy" but subtype "weakness": it
        // lives in the investigator's deck, so it prints with the player back.
        let raw = vec![
            card("08003", "Mob Goons", "enemy", Some("weakness")),
            card("01015", "Amnesia", "treachery", Some("basicweakness")),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].back_group.as_deref(), Some("player"));
        assert_eq!(cards[1].back_group.as_deref(), Some("player"));
    }

    #[test]
    fn an_unclassified_type_code_gets_no_back_group() {
        let raw = vec![card("00000", "Mystery", "investigator_choice", None)];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].back_group, None);
    }

    #[test]
    fn an_upgrade_is_a_card_of_its_own_title() {
        // Both are named "Deduction" in the API; only the level tells them
        // apart, and the pipeline groups by title.
        let raw = vec![
            player_card("01039", "Deduction", "core", 39, 0),
            player_card("02150", "Deduction", "tece", 150, 2),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].title, "Deduction");
        assert_eq!(cards[0].title_normalized, "deduction");
        assert_eq!(cards[1].title, "Deduction (2)");
        assert_ne!(cards[1].title_normalized, cards[0].title_normalized);
    }

    #[test]
    fn a_card_with_no_level_keeps_its_plain_name() {
        let raw = vec![card("01001", "Roland Banks", "investigator", None)];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].title, "Roland Banks");
    }
}
