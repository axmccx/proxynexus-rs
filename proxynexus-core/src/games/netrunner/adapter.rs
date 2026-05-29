use crate::card_source::DecklistProvider;
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::netrunner::api::{
    fetch_card_sets, fetch_cards, fetch_decklist_from_nrdb, fetch_printings,
};
use crate::models::Decklist;
use crate::mpc::CardBackProvider;
use async_trait::async_trait;

pub struct NetrunnerAdapter {}

impl Default for NetrunnerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl NetrunnerAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for NetrunnerAdapter {
    fn game_id(&self) -> &'static str {
        "netrunner"
    }

    fn game_name(&self) -> &'static str {
        "Netrunner"
    }

    async fn fetch_catalog(&self) -> Result<Catalog> {
        let nrdb_sets = fetch_card_sets().await?;
        let nrdb_cards = fetch_cards().await?;
        let nrdb_printings = fetch_printings().await?;

        let packs: Vec<Pack> = nrdb_sets
            .into_iter()
            .map(|set| Pack {
                id: set.id,
                name: set.attributes.name,
                date_release: set.attributes.date_release,
            })
            .collect();

        let cards: Vec<Card> = nrdb_cards
            .into_iter()
            .map(|card| Card {
                id: card.id,
                title: card.attributes.title.clone(),
                title_normalized: normalize_title(&card.attributes.title),
                side: Some(card.attributes.side_id),
            })
            .collect();

        let versions: Vec<CardVersion> = nrdb_printings
            .into_iter()
            .map(|printing| CardVersion {
                card_id: printing.attributes.card_id,
                pack_id: printing.attributes.card_set_id,
                quantity: printing.attributes.quantity,
                position: printing.attributes.position,
            })
            .collect();

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
impl DecklistProvider for NetrunnerAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        fetch_decklist_from_nrdb(url).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CardBackProvider for NetrunnerAdapter {
    async fn fetch_card_backs(&self) -> Result<Vec<(String, Vec<u8>)>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(vec![
                (
                    "corp_back_original.png".to_string(),
                    include_bytes!("../../../assets/corp_back_original.png").to_vec(),
                ),
                (
                    "corp_back_proxy.png".to_string(),
                    include_bytes!("../../../assets/corp_back_proxy.png").to_vec(),
                ),
                (
                    "runner_back_original.png".to_string(),
                    include_bytes!("../../../assets/runner_back_original.png").to_vec(),
                ),
                (
                    "runner_back_proxy.png".to_string(),
                    include_bytes!("../../../assets/runner_back_proxy.png").to_vec(),
                ),
            ])
        }

        #[cfg(target_arch = "wasm32")]
        {
            use futures::future::join_all;
            use gloo_net::http::Request;

            let filenames = [
                "corp_back_original.png",
                "corp_back_proxy.png",
                "runner_back_original.png",
                "runner_back_proxy.png",
            ];

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
