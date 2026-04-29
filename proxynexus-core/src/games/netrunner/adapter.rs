use crate::card_store::normalize_title;
use crate::catalog::{Card, CardVersion, Catalog, CatalogAdapter, Pack};
use crate::error::Result;
use crate::games::netrunner::models::{NrdbCard, NrdbCardSet, NrdbPrinting, NrdbResponse};
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

    async fn fetch_v3_endpoint<T: for<'de> serde::Deserialize<'de>>(
        &self,
        url: &str,
    ) -> Result<Vec<T>> {
        let mut all_data = Vec::new();
        let mut current_url = Some(url.to_string());

        while let Some(u) = current_url {
            let json_str = reqwest::get(&u).await?.text().await?;

            let response: NrdbResponse<T> = serde_json::from_str(&json_str)?;
            all_data.extend(response.data);

            current_url = response.links.and_then(|l| l.next);
        }

        Ok(all_data)
    }
}

#[async_trait]
impl CatalogAdapter for NetrunnerAdapter {
    fn game_id(&self) -> &'static str {
        "netrunner"
    }

    fn game_name(&self) -> &'static str {
        "Netrunner"
    }

    async fn fetch_catalog(&self) -> Result<Catalog> {
        let base_url = "https://api-preview.netrunnerdb.com/api/v3/public";

        let nrdb_sets: Vec<NrdbCardSet> = self
            .fetch_v3_endpoint(&format!("{}/card_sets?page[size]=1000", base_url))
            .await?;
        let nrdb_cards: Vec<NrdbCard> = self
            .fetch_v3_endpoint(&format!("{}/cards?page[size]=1000", base_url))
            .await?;
        let nrdb_printings: Vec<NrdbPrinting> = self
            .fetch_v3_endpoint(&format!("{}/printings?page[size]=1000", base_url))
            .await?;

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
