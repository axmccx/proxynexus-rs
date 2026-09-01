#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::coclcg::models::{CocCard, CocPack};
#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;

pub struct CocAdapter {}

impl Default for CocAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CocAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for CocAdapter {
    fn game_id(&self) -> &'static str {
        "coclcg"
    }

    fn game_name(&self) -> &'static str {
        "Call of Cthulhu"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["coclcg"]
    }
}

/// Story cards are the only ones with a back of their own, and each of the
/// three products holding them prints a different one, so `back_group` is
/// read off the card rather than derived here. Conspiracies look like an
/// exception but are not: they are landscape cards a player shuffles into
/// their own deck, so they carry the standard back.
#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(coc_cards: Vec<CocCard>) -> (Vec<Card>, Vec<CardVersion>) {
    let mut cards = Vec::with_capacity(coc_cards.len());
    let mut card_versions = Vec::with_capacity(coc_cards.len());

    for card in coc_cards {
        cards.push(Card {
            id: card.unique_id.clone(),
            title: card.name.clone(),
            title_normalized: normalize_title(&card.name),
            back_group: Some(card.back_group),
        });

        for version in card.versions {
            card_versions.push(CardVersion {
                card_id: card.unique_id.clone(),
                pack_id: version.pack_code,
                quantity: version.quantity,
                position: version.number,
                api_id: None,
            });
        }
    }

    (cards, card_versions)
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for CocAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let coc_packs: Vec<CocPack> = serde_json::from_str(include_str!("coc_packs.json"))?;
        let coc_cards: Vec<CocCard> = serde_json::from_str(include_str!("coc_cards.json"))?;

        let packs: Vec<Pack> = coc_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: pack.date_release,
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(coc_cards);

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
    use crate::games::coclcg::models::CocVersion;
    use std::collections::{HashMap, HashSet};

    fn version(pack_code: &str, number: i64) -> CocVersion {
        CocVersion {
            pack_code: pack_code.to_string(),
            number: Some(number),
            quantity: 3,
        }
    }

    fn card(unique_id: &str, back_group: &str, versions: Vec<CocVersion>) -> CocCard {
        CocCard {
            unique_id: unique_id.to_string(),
            name: format!("Card {unique_id}"),
            subtitle: None,
            card_type: "Character".to_string(),
            faction: "Miskatonic".to_string(),
            back_group: back_group.to_string(),
            versions,
        }
    }

    #[test]
    fn maps_unique_id_and_pack_code_onto_card_and_version() {
        let (cards, versions) =
            build_cards_and_versions(vec![card("hastur", "card", vec![version("core-set", 81)])]);

        assert_eq!(cards.len(), 1);
        assert_eq!(versions.len(), 1);
        assert_eq!(cards[0].id, "hastur");
        assert_eq!(cards[0].title, "Card hastur");
        assert_eq!(cards[0].back_group.as_deref(), Some("card"));
        assert_eq!(versions[0].card_id, "hastur");
        assert_eq!(versions[0].pack_id, "core-set");
        assert_eq!(versions[0].quantity, 3);
        assert_eq!(versions[0].position, Some(81));
    }

    #[test]
    fn a_reprint_is_one_card_with_a_version_in_each_pack() {
        let (cards, versions) = build_cards_and_versions(vec![card(
            "torch-the-joint",
            "card",
            vec![version("core-set", 18), version("spawn-of-madness", 2)],
        )]);

        assert_eq!(cards.len(), 1);
        assert_eq!(versions.len(), 2);
        for version in &versions {
            assert_eq!(version.card_id, "torch-the-joint");
        }
        assert_eq!(versions[0].position, Some(18));
        assert_eq!(versions[1].position, Some(2));
    }

    #[tokio::test]
    async fn every_card_belongs_to_a_pack_in_the_pack_list() {
        let catalog = CocAdapter::new().fetch_catalog().await.unwrap();
        let pack_ids: HashSet<&str> = catalog.packs.iter().map(|p| p.id.as_str()).collect();

        for version in &catalog.card_versions {
            assert!(
                pack_ids.contains(version.pack_id.as_str()),
                "{} is in pack '{}', which is not in coc_packs.json",
                version.card_id,
                version.pack_id
            );
        }
    }

    #[tokio::test]
    async fn card_ids_are_unique_across_the_catalog() {
        let catalog = CocAdapter::new().fetch_catalog().await.unwrap();
        let mut seen: HashSet<&str> = HashSet::new();

        for card in &catalog.cards {
            assert!(
                seen.insert(card.id.as_str()),
                "duplicate card id {}",
                card.id
            );
        }
    }

    /// Each story back belongs to one product, so a card naming one has to be
    /// printed in that product and nowhere else.
    #[tokio::test]
    async fn a_story_back_group_names_the_one_pack_its_card_is_printed_in() {
        let catalog = CocAdapter::new().fetch_catalog().await.unwrap();
        let packs_of: HashMap<&str, Vec<&str>> =
            catalog
                .card_versions
                .iter()
                .fold(HashMap::new(), |mut acc, version| {
                    acc.entry(version.card_id.as_str())
                        .or_default()
                        .push(version.pack_id.as_str());
                    acc
                });

        for card in &catalog.cards {
            let Some(group) = card.back_group.as_deref() else {
                panic!("{} has no back group", card.id);
            };
            let Some(pack_id) = group.strip_prefix("story-") else {
                assert_eq!(group, "card", "card {}", card.id);
                continue;
            };
            assert_eq!(
                packs_of.get(card.id.as_str()).map(Vec::as_slice),
                Some([pack_id].as_slice()),
                "{} names the back of '{}' but is printed elsewhere",
                card.id,
                pack_id
            );
        }
    }

    /// A back group with no image behind it falls back to a generated back
    /// without reporting anything, so the two sides are checked against each
    /// other here.
    #[tokio::test]
    async fn every_back_group_in_the_catalog_has_a_back_image() {
        let adapter = CocAdapter::new();
        let catalog = adapter.fetch_catalog().await.unwrap();

        let groups: HashSet<&str> = catalog
            .cards
            .iter()
            .filter_map(|card| card.back_group.as_deref())
            .collect();

        for group in groups {
            assert!(
                crate::card_backs::card_back(adapter.game_id(), group, "original").is_some(),
                "no backs/{}_original image for back group '{}'",
                group,
                group
            );
        }
    }
}
