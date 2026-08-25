use crate::card_source::DecklistProvider;
#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::GameAdapterInfo;
use crate::games::agot::api::fetch_decklist_from_thronesdb;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::agot::api::{fetch_all_cards, fetch_all_packs};
use crate::models::Decklist;
use async_trait::async_trait;

pub struct AgotAdapter {}

impl Default for AgotAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgotAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for AgotAdapter {
    fn game_id(&self) -> &'static str {
        "agot"
    }

    fn game_name(&self) -> &'static str {
        "A Game of Thrones"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["thrones", "agot"]
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for AgotAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let api_packs = fetch_all_packs().await?;
        let api_cards = fetch_all_cards().await?;

        let packs: Vec<Pack> = api_packs
            .into_iter()
            .map(|p| Pack {
                id: p.code,
                name: p.name,
                date_release: p.available,
            })
            .collect();

        // In AGOT, different cards can have the same name.
        // ThronesDB uses the 'label' field to distinguish them.
        // We use normalized label as the Card ID to keep functional versions distinct.
        let mut cards_map = std::collections::HashMap::new();
        let mut versions = Vec::new();

        for c in api_cards {
            let normalized_id = normalize_title(&c.label);

            // Side mapping for MPC grouping
            let back_group = match c.type_code.as_str() {
                "plot" => "plot",
                "agenda" => "agenda",
                "title" => "title",
                _ => "draw",
            };

            if !cards_map.contains_key(&normalized_id) {
                cards_map.insert(
                    normalized_id.clone(),
                    Card {
                        id: normalized_id.clone(),
                        title: c.label.clone(), // Use label as title for clarity
                        title_normalized: normalized_id.clone(),
                        back_group: Some(back_group.to_string()),
                    },
                );
            }

            versions.push(CardVersion {
                card_id: normalized_id,
                pack_id: c.pack_code,
                quantity: c.quantity,
                position: c.position,
                api_id: None,
            });
        }

        let cards: Vec<Card> = cards_map.into_values().collect();

        Ok(Catalog {
            game_id: self.game_id().to_string(),
            display_name: self.game_name().to_string(),
            packs,
            cards,
            card_versions: versions,
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DecklistProvider for AgotAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        fetch_decklist_from_thronesdb(url).await
    }
}
