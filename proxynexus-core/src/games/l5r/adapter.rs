use crate::card_source::DecklistProvider;
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::l5r::api::fetch_decklist_from_emeralddb;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::l5r::api::{fetch_cards, fetch_packs};
use crate::models::Decklist;
use crate::mpc::CardBackProvider;
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use futures::join;

pub struct L5rAdapter {}

impl Default for L5rAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl L5rAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for L5rAdapter {
    fn game_id(&self) -> &'static str {
        "l5r"
    }

    fn game_name(&self) -> &'static str {
        "Legend of the Five Rings"
    }

    async fn fetch_catalog(&self) -> Result<Catalog> {
        let (cards_result, packs_result) = join!(fetch_cards(), fetch_packs());
        let l5r_cards = cards_result?;
        let l5r_packs = packs_result?;

        let packs: Vec<Pack> = l5r_packs
            .into_iter()
            .map(|p| Pack {
                id: p.id,
                name: p.name,
                date_release: p.released_at,
            })
            .collect();

        let mut cards: Vec<Card> = Vec::new();
        let mut versions: Vec<CardVersion> = Vec::new();
        for c in l5r_cards {
            let title = build_title(&c.name, c.name_extra.as_deref());
            cards.push(Card {
                id: c.id.clone(),
                title: title.clone(),
                title_normalized: normalize_title(&title),
                side: Some(c.side),
            });
            for v in c.versions {
                versions.push(CardVersion {
                    card_id: v.card_id,
                    pack_id: v.pack_id,
                    quantity: v.quantity,
                    position: parse_position(v.position.as_deref()),
                });
            }
        }

        Ok(Catalog {
            game_id: self.game_id().to_string(),
            display_name: self.game_name().to_string(),
            packs,
            cards,
            card_versions: versions,
        })
    }
}

fn build_title(name: &str, name_extra: Option<&str>) -> String {
    match name_extra {
        Some(extra) if !extra.is_empty() => format!("{} ({})", name, extra),
        _ => name.to_string(),
    }
}

fn parse_position(s: Option<&str>) -> Option<i64> {
    s.and_then(|v| v.parse::<i64>().ok())
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DecklistProvider for L5rAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        fetch_decklist_from_emeralddb(url).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CardBackProvider for L5rAdapter {
    async fn fetch_card_backs(&self) -> Result<Vec<(String, Vec<u8>)>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(vec![
                (
                    "conflict_back.png".to_string(),
                    include_bytes!("../../../assets/conflict_back.png").to_vec(),
                ),
                (
                    "dynasty_back.png".to_string(),
                    include_bytes!("../../../assets/dynasty_back.png").to_vec(),
                ),
                (
                    "province_back.png".to_string(),
                    include_bytes!("../../../assets/province_back.png").to_vec(),
                ),
            ])
        }

        #[cfg(target_arch = "wasm32")]
        {
            use futures::future::join_all;
            use gloo_net::http::Request;

            let filenames = ["conflict_back.png", "dynasty_back.png", "province_back.png"];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_title_without_extra_returns_name_only() {
        assert_eq!(build_title("A Bad Death", None), "A Bad Death");
    }

    #[test]
    fn build_title_with_empty_extra_returns_name_only() {
        assert_eq!(build_title("A Bad Death", Some("")), "A Bad Death");
    }

    #[test]
    fn build_title_with_extra_appends_parenthesized_suffix() {
        assert_eq!(
            build_title("A Fate Worse Than Death", Some("2")),
            "A Fate Worse Than Death (2)"
        );
    }

    #[test]
    fn parse_position_returns_some_for_numeric_string() {
        assert_eq!(parse_position(Some("168")), Some(168));
    }

    #[test]
    fn parse_position_returns_none_for_none_input() {
        assert_eq!(parse_position(None), None);
    }

    #[test]
    fn parse_position_returns_none_for_non_numeric_string() {
        assert_eq!(parse_position(Some("xyz")), None);
    }

    #[test]
    fn parse_position_returns_none_for_empty_string() {
        assert_eq!(parse_position(Some("")), None);
    }
}
