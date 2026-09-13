#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::agot1st::models::{Agot1stCard, Agot1stPack};
#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;

/// There is one card back for A Game of Thrones 1st Edition,
/// even though the players have two decks (the draw deck and the plot deck).
#[cfg(not(target_arch = "wasm32"))]
const AGOT1ST_BACK_GROUP: &str = "card";

pub struct Agot1stAdapter {}

impl Default for Agot1stAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Agot1stAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for Agot1stAdapter {
    fn game_id(&self) -> &'static str {
        "agot1st"
    }

    fn game_name(&self) -> &'static str {
        "A Game of Thrones 1st Edition"
    }

}

/// Turns the flat `agot1st_cards.json` card list into catalog rows.
///
/// No double-sided cards exist in this game, all cards use the
/// same back.
#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(
    agot1st_cards: Vec<crate::games::agot1st::models::Agot1stCard>,
) -> (Vec<Card>, Vec<CardVersion>) {
    let mut cards = Vec::with_capacity(agot1st_cards.len());
    let mut card_versions = Vec::with_capacity(agot1st_cards.len());

    for card in agot1st_cards {
        cards.push(Card {
            id: card.unique_id.clone(),
            title: card.name.clone(),
            title_normalized: normalize_title(&card.name),
            back_group: Some(AGOT1ST_BACK_GROUP.to_string()),
        });

        card_versions.push(CardVersion {
            card_id: card.unique_id,
            pack_id: card.pack_code,
            quantity: card.card_quantity.unwrap_or(1),
            position: card.card_number,
            api_id: None,
        });
    }

    (cards, card_versions)
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for Agot1stAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        // Load all packs (sets/expansions). All data is stored in a
        // single JSON file.
        let agot1st_packs: Vec<Agot1stPack> =
            serde_json::from_str(include_str!("agot1st_packs.json"))?;

        // Load every card across every pack. `agot1st_full.json` is one bulk
        // file covering the whole catalog.
        let agot1st_cards: Vec<Agot1stCard> =
            serde_json::from_str(include_str!("agot1st_cards.json"))?;

        let packs: Vec<Pack> = agot1st_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: pack.available,
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(agot1st_cards);

        Ok(Catalog {
            game_id: self.game_id().to_string(),
            display_name: self.game_name().to_string(),
            packs,
            cards,
            card_versions,
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::games::agot1st::models::Agot1stCard;

    fn card(
        unique_id: &str,
        name: &str,
        pack_code: &str,
        card_type: &str,
        house: &str,
    ) -> Agot1stCard {
        Agot1stCard {
            unique_id: unique_id.to_string(),
            name: name.to_string(),
            pack_code: pack_code.to_string(),
            card_type: card_type.to_string(),
            house: house.to_string(),
            card_quantity: Some(3),
            card_number: Some(1),
        }
    }

    #[test]
    fn maps_unique_id_and_pack_code_onto_card_and_version() {
        // 16120, in the real catalog, is "Favored by the Warrior" from
        // A Journey's End (pack_code a-journey-s-end).
        let (cards, versions) = build_cards_and_versions(vec![card(
            "16120",
            "Favored by the Warrior",
            "a-journey-s-end",
            "Event",
            "Neutral",
        )]);

        assert_eq!(cards.len(), 1);
        assert_eq!(versions.len(), 1);
        assert_eq!(cards[0].id, "16120");
        assert_eq!(cards[0].title, "Favored by the Warrior");
        assert_eq!(cards[0].back_group.as_deref(), Some("card"));
        assert_eq!(versions[0].card_id, "16120");
        assert_eq!(versions[0].pack_id, "a-journey-s-end");
    }

    #[test]
    fn card_quantity_is_carried_through_when_present() {
        let mut raw = card("13025", "Card 13025", "core-set", "Character", "Lannister");
        raw.card_quantity = Some(3);
        let (_, versions) = build_cards_and_versions(vec![raw]);

        assert_eq!(versions[0].quantity, 3);
    }

    #[test]
    fn card_number_is_carried_through_to_version_position() {
        let mut raw = card("14115", "Card 14115", "core-set", "Attachment", "Martell");
        raw.card_number = Some(115);
        let (_, versions) = build_cards_and_versions(vec![raw]);

        assert_eq!(versions[0].position, Some(115));
    }
}
