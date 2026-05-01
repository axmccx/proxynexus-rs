use crate::card_source::DecklistProvider;
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogAdapter, Pack};
use crate::error::Result;
use crate::games::netrunner::api::{
    fetch_card_sets, fetch_cards, fetch_decklist_from_nrdb, fetch_printings,
};
use crate::models::Decklist;
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
impl CatalogAdapter for NetrunnerAdapter {
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
