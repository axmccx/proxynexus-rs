#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::whinvasion::models::{WhiCard, WhiPack};
use crate::mpc::CardBackProvider;
use async_trait::async_trait;

pub struct WhiAdapter {}

impl Default for WhiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WhiAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for WhiAdapter {
    fn game_id(&self) -> &'static str {
        "whinvasion"
    }

    fn game_name(&self) -> &'static str {
        "Warhammer Invasion"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["whinvasion"]
    }
}

/// Turns the flat `whi_full.json` card list into catalog rows.
///
/// No double-sided cards exist in this game -- confirmed against the
/// actual collection (1133 cards, 1133 images, zero `~back` files), so
/// each `WhiCard` maps 1:1 to one physical card with a single generic
/// back (see `WhiAdapter::fetch_card_backs`).
#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(
    whi_cards: Vec<crate::games::whinvasion::models::WhiCard>,
) -> (Vec<Card>, Vec<CardVersion>) {
    let mut cards = Vec::with_capacity(whi_cards.len());
    let mut card_versions = Vec::with_capacity(whi_cards.len());

    for card in whi_cards {
        cards.push(Card {
            id: card.unique_id.clone(),
            title: card.name.clone(),
            title_normalized: normalize_title(&card.name),
            back_group: Some(card.race),
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
impl CatalogProvider for WhiAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        // Load all packs (sets/expansions). All data is stored in a
        // single JSON file.
        let whi_packs: Vec<WhiPack> = serde_json::from_str(include_str!("whi_packs.json"))?;

        // Load every card across every pack. `whi_full.json` is one bulk
        // file covering the whole catalog.
        let whi_cards: Vec<WhiCard> = serde_json::from_str(include_str!("whi_full.json"))?;

        let packs: Vec<Pack> = whi_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: pack.available,
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(whi_cards);

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
impl CardBackProvider for WhiAdapter {
    async fn fetch_card_backs(&self) -> Result<Vec<(String, Vec<u8>)>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(vec![(
                "warhammer_invasion_card_back.png".to_string(),
                include_bytes!("../../../assets/warhammer_invasion_card_back.png").to_vec(),
            )])
        }

        #[cfg(target_arch = "wasm32")]
        {
            use futures::future::join_all;
            use gloo_net::http::Request;

            let filenames = ["warhammer_invasion_card_back.png"];

            let fetch_futures = filenames.iter().map(|filename| async move {
                let url = format!("card_backs/{}", filename);
                let response = Request::get(&url).send().await?;

                if !response.ok() {
                    return Err(crate::error::ProxyNexusError::Internal(format!(
                        "Failed to fetch {}: HTTP {}",
                        url,
                        response.status()
                    )));
                }

                let bytes = response.binary().await?;

                Ok((filename.to_string(), bytes))
            });

            let results: Vec<Result<(String, Vec<u8>)>> = join_all(fetch_futures).await;
            results.into_iter().collect()
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::games::whinvasion::models::WhiCard;

    fn card(unique_id: &str, card_type: &str, race: &str) -> WhiCard {
        WhiCard {
            unique_id: unique_id.to_string(),
            name: format!("Card {unique_id}"),
            pack_code: "core-set".to_string(),
            card_type: card_type.to_string(),
            race: race.to_string(),
            card_quantity: Some(3),
            card_number: Some(1),
        }
    }

    #[test]
    fn maps_unique_id_pack_code_and_race_onto_card_and_version() {
        let (cards, versions) = build_cards_and_versions(vec![card("10013", "Unit", "Orc")]);

        assert_eq!(cards.len(), 1);
        assert_eq!(versions.len(), 1);
        assert_eq!(cards[0].id, "10013");
        assert_eq!(cards[0].title, "Card 10013");
        assert_eq!(cards[0].back_group.as_deref(), Some("Orc"));
        assert_eq!(versions[0].card_id, "10013");
        assert_eq!(versions[0].pack_id, "core-set");
    }

    #[test]
    fn missing_card_quantity_defaults_to_one() {
        let mut raw = card("01017", "Quest", "Dwarf");
        raw.card_quantity = None;
        let (_, versions) = build_cards_and_versions(vec![raw]);

        assert_eq!(versions[0].quantity, 1);
    }

    #[test]
    fn card_quantity_is_carried_through_when_present() {
        let mut raw = card("04098", "Tactic", "Neutral");
        raw.card_quantity = Some(3);
        let (_, versions) = build_cards_and_versions(vec![raw]);

        assert_eq!(versions[0].quantity, 3);
    }

    #[test]
    fn card_number_is_carried_through_to_version_position() {
        let mut raw = card("08118", "Tactic", "Neutral");
        raw.card_number = Some(118);
        let (_, versions) = build_cards_and_versions(vec![raw]);

        assert_eq!(versions[0].position, Some(118));
    }
}
