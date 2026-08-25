use crate::card_source::DecklistProvider;
#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::GameAdapterInfo;
use crate::games::netrunner::adapter::NetrunnerAdapter;
use crate::games::netrunner_reboot::api::fetch_decklist_from_reteki;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::netrunner_reboot::api::{fetch_all_cards, fetch_all_packs};
use crate::models::Decklist;
use crate::mpc::CardBackProvider;
use async_trait::async_trait;

pub struct NetrunnerRebootAdapter {}

impl Default for NetrunnerRebootAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl NetrunnerRebootAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for NetrunnerRebootAdapter {
    fn game_id(&self) -> &'static str {
        "netrunner-reboot"
    }

    fn game_name(&self) -> &'static str {
        "Netrunner Reboot Project"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["netrunner-reboot"]
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for NetrunnerRebootAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let api_packs = fetch_all_packs().await?;
        let api_cards = fetch_all_cards().await?;

        let packs: Vec<Pack> = api_packs
            .into_iter()
            .map(|p| Pack {
                id: p.code,
                name: p.name,
                date_release: p.date_release,
            })
            .collect();

        // reteki runs the NetrunnerDB v2 API, where a card and its printing are
        // one object. A card reprinted in a second pack would therefore arrive
        // twice under the same id, so cards are collected by id and only the
        // printings are allowed to repeat.
        let mut cards_map = std::collections::HashMap::new();
        let mut versions = Vec::new();

        for c in api_cards {
            let card_id = normalize_title(&c.title);

            cards_map.entry(card_id.clone()).or_insert_with(|| Card {
                id: card_id.clone(),
                title: c.title.clone(),
                title_normalized: card_id.clone(),
                back_group: Some(c.side_code),
            });

            versions.push(CardVersion {
                card_id,
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
impl DecklistProvider for NetrunnerRebootAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        fetch_decklist_from_reteki(url).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CardBackProvider for NetrunnerRebootAdapter {
    async fn fetch_card_backs(&self) -> Result<Vec<(String, Vec<u8>)>> {
        // Reboot reprints FFG-era cards, so the backs are the Netrunner ones.
        NetrunnerAdapter::new().fetch_card_backs().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every accented title in the reteki catalog. Image filenames are written
    /// by utils/image_file_renamers/netrunner_reboot/, which reimplements
    /// normalize_title in Python; a card whose id the two sides disagree on
    /// would be filed as a custom variant instead of an official printing.
    #[test]
    fn accented_titles_normalize_to_the_ids_the_renamer_writes() {
        for (title, id) in [
            ("Déjà Vu", "deja_vu"),
            ("Dracō", "draco"),
            ("Chaos Theory: Wünderkind", "chaos_theory__wunderkind"),
            ("Doppelgänger", "doppelganger"),
            ("Shi.Kyū", "shi_kyu"),
            ("Tori Hanzō", "tori_hanzo"),
            ("Exposé", "expose"),
        ] {
            assert_eq!(normalize_title(title), id, "title: {}", title);
        }
    }
}
