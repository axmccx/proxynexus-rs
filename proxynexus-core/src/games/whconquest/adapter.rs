#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::whconquest::models::{WhcCard, WhcPack};
#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;

/// Every Conquest card carries the same back, so the game has one back group.
/// Planets look like an exception but are not: they are ordinary portrait
/// cards whose art is printed sideways, not landscape cards with a reverse of
/// their own. Warlords are double-sided, but their reverse is the bloodied
/// side, which arrives as a `~back` scan rather than a shared card back.
#[cfg(not(target_arch = "wasm32"))]
const WHC_BACK_GROUP: &str = "card";

pub struct WhcAdapter {}

impl Default for WhcAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WhcAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for WhcAdapter {
    fn game_id(&self) -> &'static str {
        "whconquest"
    }

    fn game_name(&self) -> &'static str {
        "Warhammer 40k Conquest"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["whconquest"]
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(whc_cards: Vec<WhcCard>) -> (Vec<Card>, Vec<CardVersion>) {
    let mut cards = Vec::with_capacity(whc_cards.len());
    let mut card_versions = Vec::with_capacity(whc_cards.len());

    for card in whc_cards {
        let title = card.name.trim();
        cards.push(Card {
            id: card.unique_id.clone(),
            title: title.to_string(),
            title_normalized: normalize_title(title),
            back_group: Some(WHC_BACK_GROUP.to_string()),
        });

        card_versions.push(CardVersion {
            card_id: card.unique_id,
            pack_id: card.pack_code,
            quantity: card.card_quantity,
            position: card.card_number,
            api_id: None,
        });
    }

    (cards, card_versions)
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for WhcAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let whc_packs: Vec<WhcPack> = serde_json::from_str(include_str!("whc_packs.json"))?;
        let whc_cards: Vec<WhcCard> = serde_json::from_str(include_str!("whc_cards.json"))?;

        let packs: Vec<Pack> = whc_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: pack.date_release,
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(whc_cards);

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
    use std::collections::HashSet;

    fn card(unique_id: &str, card_type: &str) -> WhcCard {
        WhcCard {
            unique_id: unique_id.to_string(),
            name: format!("Card {unique_id}"),
            pack_code: "core-set".to_string(),
            card_type: card_type.to_string(),
            faction: "Ork".to_string(),
            card_number: Some(1),
            card_quantity: 3,
        }
    }

    #[test]
    fn maps_unique_id_and_pack_code_onto_card_and_version() {
        let (cards, versions) = build_cards_and_versions(vec![card("acid-maw", "Army")]);

        assert_eq!(cards.len(), 1);
        assert_eq!(versions.len(), 1);
        assert_eq!(cards[0].id, "acid-maw");
        assert_eq!(cards[0].title, "Card acid-maw");
        assert_eq!(cards[0].back_group.as_deref(), Some("card"));
        assert_eq!(versions[0].card_id, "acid-maw");
        assert_eq!(versions[0].pack_id, "core-set");
        assert_eq!(versions[0].quantity, 3);
        assert_eq!(versions[0].position, Some(1));
    }

    #[test]
    fn card_names_are_trimmed_before_they_are_normalized() {
        let mut whc_card = card("broadside", "Army");
        whc_card.name = "Sa'cea XV88 Broadside ".to_string();
        let (cards, _) = build_cards_and_versions(vec![whc_card]);

        assert_eq!(cards[0].title, "Sa'cea XV88 Broadside");
        assert_eq!(
            cards[0].title_normalized,
            normalize_title("Sa'cea XV88 Broadside")
        );
    }

    #[test]
    fn every_card_type_shares_the_one_card_back() {
        let (cards, _) = build_cards_and_versions(vec![
            card("plannum", "Planet"),
            card("nahumekh", "Warlord"),
            card("acid-maw", "Army"),
        ]);

        for card in &cards {
            assert_eq!(card.back_group.as_deref(), Some("card"), "card {}", card.id);
        }
    }

    #[tokio::test]
    async fn every_card_belongs_to_a_pack_in_the_pack_list() {
        let catalog = WhcAdapter::new().fetch_catalog().await.unwrap();
        let pack_ids: HashSet<&str> = catalog.packs.iter().map(|p| p.id.as_str()).collect();

        for version in &catalog.card_versions {
            assert!(
                pack_ids.contains(version.pack_id.as_str()),
                "{} is in pack '{}', which is not in whc_packs.json",
                version.card_id,
                version.pack_id
            );
        }
    }

    #[tokio::test]
    async fn card_ids_are_unique_across_the_catalog() {
        let catalog = WhcAdapter::new().fetch_catalog().await.unwrap();
        let mut seen: HashSet<&str> = HashSet::new();

        for card in &catalog.cards {
            assert!(
                seen.insert(card.id.as_str()),
                "duplicate card id {}",
                card.id
            );
        }
    }
}
