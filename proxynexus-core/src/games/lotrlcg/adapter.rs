#[cfg(not(target_arch = "wasm32"))]
use super::models::HobCard;
use crate::card_source::DecklistProvider;
#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::ProxyNexusError;
use crate::error::Result;
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::fetch_json;
use crate::games::lotrlcg::api::fetch_decklist_from_ringsdb;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::lotrlcg::back_group_from_type_code;
use crate::models::Decklist;
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::{HashMap, HashSet};

pub struct LotrLcgAdapter;

impl LotrLcgAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LotrLcgAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GameAdapterInfo for LotrLcgAdapter {
    fn game_id(&self) -> &'static str {
        "lotrlcg"
    }

    fn game_name(&self) -> &'static str {
        "Lord of the Rings LCG"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["lotrlcg"]
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DecklistProvider for LotrLcgAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        fetch_decklist_from_ringsdb(url).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ffg_release_dates() -> Result<Vec<(String, String)>> {
    #[derive(serde::Deserialize)]
    struct Release {
        released: String,
    }

    let raw = include_str!("ffg_release_dates.json");
    let by_set: std::collections::BTreeMap<String, Release> = serde_json::from_str(raw)
        .map_err(|e| ProxyNexusError::Internal(format!("ffg_release_dates.json: {}", e)))?;
    Ok(by_set
        .into_iter()
        .map(|(card_set, release)| (card_set, release.released))
        .collect())
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for LotrLcgAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let player_cards_url = "http://hallofbeorn.com/Export/PlayerCards";
        let encounter_cards_url = "http://hallofbeorn.com/Export/EncounterCards";
        let quest_cards_url = "http://hallofbeorn.com/Export/QuestCards";

        let mut all_hob_cards: Vec<HobCard> = fetch_json(player_cards_url).await?;
        let mut encounter_cards: Vec<HobCard> = fetch_json(encounter_cards_url).await?;
        let mut quest_cards: Vec<HobCard> = fetch_json(quest_cards_url).await?;

        all_hob_cards.append(&mut encounter_cards);
        all_hob_cards.append(&mut quest_cards);

        let mut packs = Vec::new();
        let mut seen_pack_names = HashSet::new();

        let mut pack_dates = HashMap::new();
        for rp in crate::games::lotrlcg::api::fetch_ringsdb_packs().await? {
            let clean_pack_name = crate::games::lotrlcg::canonical_pack_name(&rp.name);
            let clean_pack_id = normalize_title(&clean_pack_name);
            pack_dates.insert(clean_pack_id, rp.available);
        }
        for (card_set, released) in ffg_release_dates()? {
            pack_dates.insert(normalize_title(&card_set), released);
        }

        let card_ids =
            crate::games::lotrlcg::identity::printing_card_ids(&all_hob_cards, &pack_dates);
        let card_titles = crate::games::lotrlcg::identity::card_titles(&all_hob_cards, &card_ids);
        tracing::debug!(
            "lotrlcg: {} Hall of Beorn printings resolve to {} distinct cards",
            card_ids.len(),
            card_titles.len()
        );

        for c in &all_hob_cards {
            let clean_pack_id = normalize_title(&c.card_set);
            if seen_pack_names.insert(clean_pack_id.clone()) {
                packs.push(Pack {
                    id: clean_pack_id.clone(),
                    name: c.card_set.clone(),
                    date_release: pack_dates.get(&clean_pack_id).cloned(),
                });
            }
        }

        let mut cards = Vec::new();
        let mut card_versions = Vec::new();
        let mut seen_cards = HashSet::new();
        let mut seen_versions = HashSet::new();
        let mut provided_pack_positions = HashSet::new();

        for c in all_hob_cards {
            let clean_pack_id = normalize_title(&c.card_set);
            let slug = normalize_title(&c.slug);
            let card_id = card_ids.get(&slug).cloned().unwrap_or_else(|| slug.clone());
            let title = card_titles
                .get(&card_id)
                .cloned()
                .unwrap_or_else(|| c.title.clone());
            let base_normalized = normalize_title(&title);

            let back_group = match c.card_type.as_str() {
                "Ally" | "Attachment" | "Contract" | "Event" | "Hero" | "Player_Side_Quest"
                | "Treasure" => "player",
                "Quest" | "Campaign" | "GenCon_Setup" | "Nightmare_Setup" => "quest",
                _ => "encounter", // Encounter_Side_Quest, Enemy, Location, Objective, Objective_Ally, Objective_Hero, Objective_Location, Ship_Enemy, Ship_Objective, Treachery, etc.
            };

            if seen_cards.insert(card_id.clone()) {
                cards.push(Card {
                    id: card_id.clone(),
                    title,
                    title_normalized: base_normalized,
                    back_group: Some(back_group.to_string()),
                });
            }

            if seen_versions.insert((slug.clone(), clean_pack_id.clone())) {
                provided_pack_positions.insert((clean_pack_id.clone(), c.number));
                card_versions.push(CardVersion {
                    card_id,
                    pack_id: clean_pack_id,
                    quantity: c.quantity.unwrap_or(3),
                    position: Some(c.number),
                    api_id: Some(slug),
                });
            }
        }

        let alep_cards = crate::games::lotrlcg::api::fetch_alep_catalog().await?;
        for rc in alep_cards {
            if rc.is_official.unwrap_or(true) {
                continue;
            }

            let base_normalized = normalize_title(&rc.name);
            let clean_pack_name = crate::games::lotrlcg::canonical_pack_name(&rc.pack_name);
            let display_name = format!("ALeP - {}", clean_pack_name);
            let clean_pack_id = normalize_title(&clean_pack_name);
            let normalized_id = normalize_title(&format!("{}-{}", rc.name, clean_pack_id));

            if seen_pack_names.insert(clean_pack_id.clone()) {
                packs.push(Pack {
                    id: clean_pack_id.clone(),
                    name: display_name.clone(),
                    date_release: pack_dates.get(&clean_pack_id).cloned(),
                });
            } else if let Some(pack) = packs
                .iter_mut()
                .find(|p| p.id == clean_pack_id && !p.name.starts_with("ALeP - "))
            {
                pack.name = display_name;
            }

            let back_group = back_group_from_type_code(rc.type_code.as_deref());

            if seen_cards.insert(normalized_id.clone()) {
                cards.push(Card {
                    id: normalized_id.clone(),
                    title: rc.name,
                    title_normalized: base_normalized,
                    back_group: Some(back_group.to_string()),
                });
            }

            if seen_versions.insert((normalized_id.clone(), clean_pack_id.clone())) {
                if let Some(pos) = rc.position {
                    provided_pack_positions.insert((clean_pack_id.clone(), pos as i64));
                }
                card_versions.push(CardVersion {
                    card_id: normalized_id,
                    pack_id: clean_pack_id,
                    quantity: rc.quantity.unwrap_or(3) as i64,
                    position: rc.position.map(|p| p as i64),
                    api_id: None,
                });
            }
        }

        let ringsdb_cards = crate::games::lotrlcg::api::fetch_all_cards().await?;
        for rc in ringsdb_cards {
            let base_normalized = normalize_title(&rc.name);

            let is_alep = rc.pack_name.replace(".English", "").starts_with("ALeP - ");
            let clean_pack_name = crate::games::lotrlcg::canonical_pack_name(&rc.pack_name);

            let display_name = if is_alep {
                format!("ALeP - {}", clean_pack_name)
            } else {
                clean_pack_name.clone()
            };

            let clean_pack_id = normalize_title(&clean_pack_name);
            let normalized_id = normalize_title(&format!("{}-{}", rc.name, clean_pack_id));

            if seen_pack_names.insert(clean_pack_id.clone()) {
                packs.push(Pack {
                    id: clean_pack_id.clone(),
                    name: display_name.clone(),
                    date_release: pack_dates.get(&clean_pack_id).cloned(),
                });
            } else if let Some(pack) = packs.iter_mut().find(|p| p.id == clean_pack_id) {
                if is_alep && !pack.name.starts_with("ALeP - ") {
                    pack.name = display_name;
                }
                if pack.date_release.is_none() {
                    pack.date_release = pack_dates.get(&clean_pack_id).cloned();
                }
            }

            let back_group = back_group_from_type_code(rc.type_code.as_deref());

            if rc.position.is_some_and(|pos| {
                provided_pack_positions.contains(&(clean_pack_id.clone(), pos as i64))
            }) {
                continue;
            }

            if seen_cards.insert(normalized_id.clone()) {
                cards.push(Card {
                    id: normalized_id.clone(),
                    title: rc.name,
                    title_normalized: base_normalized,
                    back_group: Some(back_group.to_string()),
                });
            }

            if seen_versions.insert((normalized_id.clone(), clean_pack_id.clone())) {
                if let Some(pos) = rc.position {
                    provided_pack_positions.insert((clean_pack_id.clone(), pos as i64));
                }
                card_versions.push(CardVersion {
                    card_id: normalized_id,
                    pack_id: clean_pack_id,
                    quantity: rc.quantity.unwrap_or(3) as i64,
                    position: rc.position.map(|p| p as i64),
                    api_id: None,
                });
            }
        }

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

    #[test]
    fn the_committed_release_dates_parse_and_cover_the_ffg_line() {
        let dates = ffg_release_dates().expect("ffg_release_dates.json should parse");
        // 109 non-Nightmare card sets; a drop here means the file was truncated.
        assert!(
            dates.len() >= 100,
            "expected the whole FFG line, got {}",
            dates.len()
        );
        assert!(dates.iter().all(|(_, released)| released.len() == 10));
    }

    #[test]
    fn the_dark_of_mirkwood_is_dated_by_its_product_not_its_contents() {
        // The reason this file exists. RingsDB dates this pack 2011-04-22,
        // which makes it the canonical printing for 77 cards it merely
        // reprints; FFG released it in December 2021.
        let dates = ffg_release_dates().unwrap();
        let found = dates
            .iter()
            .find(|(card_set, _)| card_set == "The Dark of Mirkwood")
            .expect("The Dark of Mirkwood should be dated");
        assert_eq!(found.1, "2021-12-01");
    }

    #[test]
    fn ffg_dates_win_over_ringsdb_and_leave_other_packs_alone() {
        let mut pack_dates = HashMap::new();
        pack_dates.insert("the_dark_of_mirkwood".to_string(), "2011-04-22".to_string());
        pack_dates.insert(
            "alep_the_aldburg_plot".to_string(),
            "2021-03-04".to_string(),
        );

        for (card_set, released) in ffg_release_dates().unwrap() {
            pack_dates.insert(normalize_title(&card_set), released);
        }

        assert_eq!(pack_dates["the_dark_of_mirkwood"], "2021-12-01");
        // ALeP is still publishing, so it keeps whatever RingsDB reported.
        assert_eq!(pack_dates["alep_the_aldburg_plot"], "2021-03-04");
    }
}
