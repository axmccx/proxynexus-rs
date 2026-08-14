#[cfg(not(target_arch = "wasm32"))]
use super::models::HobCard;
use crate::card_source::DecklistProvider;
#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::fetch_json;
use crate::games::lotrlcg::api::fetch_decklist_from_ringsdb;
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

/// The RingsDB pack a card really belongs to: its id less the 3-digit number.
#[cfg(not(target_arch = "wasm32"))]
fn ringsdb_pack_prefix(card: &HobCard) -> Option<&str> {
    let id = card.rings_db_card_id.as_deref()?;
    (id.len() > 3).then(|| &id[..id.len() - 3])
}

/// Slugs of promo cards Hall of Beorn files inside another product's card set.
///
/// Gen Con and preorder promo heroes were handed out alongside a scenario, and
/// the catalog lists them as part of that scenario even though the box does not
/// contain them. Their `RingsDbCardId` still points at the pack they were really
/// printed in, so within a set they are the cards whose pack prefix disagrees
/// with everyone else's.
///
/// Sets with no clear majority prefix are left alone. The hero and starter
/// products -- Defenders of Gondor, Elves of Lorien, the Two-Player Starter --
/// are collections of reprints, where a mixed prefix is the honest answer.
#[cfg(not(target_arch = "wasm32"))]
fn promo_slugs(cards: &[HobCard]) -> HashSet<String> {
    const DOMINANT_SHARE: f64 = 0.6;

    let mut by_set: HashMap<&str, Vec<(&str, &HobCard)>> = HashMap::new();
    for card in cards {
        if let Some(prefix) = ringsdb_pack_prefix(card) {
            by_set
                .entry(card.card_set.as_str())
                .or_default()
                .push((prefix, card));
        }
    }

    let mut promos = HashSet::new();
    for members in by_set.values() {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for (prefix, _) in members {
            *counts.entry(prefix).or_default() += 1;
        }

        // Sorted so a tie resolves the same way on every run.
        let mut ranked: Vec<_> = counts.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let Some(&(dominant, hits)) = ranked.first() else {
            continue;
        };
        if (hits as f64) < members.len() as f64 * DOMINANT_SHARE {
            continue;
        }

        for (prefix, card) in members {
            if *prefix != dominant {
                promos.insert(card.slug.clone());
            }
        }
    }
    promos
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

        // Dropped before anything else reads the export, so promos never reach
        // the catalog and their scans resolve to no card. rename.py applies the
        // same rule in find_promo_slugs.
        let promos = promo_slugs(&all_hob_cards);
        all_hob_cards.retain(|c| !promos.contains(&c.slug));

        let mut packs = Vec::new();
        let mut seen_pack_names = HashSet::new();

        // Build a mapping of normalized pack name to release date from RingsDB.
        let mut pack_dates = HashMap::new();
        for rp in crate::games::lotrlcg::api::fetch_ringsdb_packs().await? {
            let clean_pack_name = rp.name.replace("ALeP - ", "").replace(".English", "");
            let clean_pack_id = normalize_title(&clean_pack_name);
            pack_dates.insert(clean_pack_id, rp.available);
        }

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
            let base_normalized = normalize_title(&c.title);
            let clean_pack_id = normalize_title(&c.card_set);
            let normalized_id = normalize_title(&c.slug);
            let title = c.title.clone();

            let side = match c.card_type.as_str() {
                "Ally" | "Attachment" | "Contract" | "Event" | "Hero" | "Player_Side_Quest"
                | "Treasure" => "player",
                "Quest" | "Campaign" | "GenCon_Setup" | "Nightmare_Setup" => "quest",
                _ => "encounter", // Encounter_Side_Quest, Enemy, Location, Objective, Objective_Ally, Objective_Hero, Objective_Location, Ship_Enemy, Ship_Objective, Treachery, etc.
            };

            if seen_cards.insert(normalized_id.clone()) {
                cards.push(Card {
                    id: normalized_id.clone(),
                    title,
                    title_normalized: base_normalized,
                    side: Some(side.to_string()),
                });
            }

            if seen_versions.insert((normalized_id.clone(), clean_pack_id.clone())) {
                provided_pack_positions.insert((clean_pack_id.clone(), c.number));
                card_versions.push(CardVersion {
                    card_id: normalized_id,
                    pack_id: clean_pack_id,
                    quantity: c.quantity.unwrap_or(3),
                    position: Some(c.number),
                });
            }
        }

        let alep_cards = crate::games::lotrlcg::api::fetch_alep_catalog().await?;
        for rc in alep_cards {
            if rc.is_official.unwrap_or(true) {
                continue;
            }

            let base_normalized = normalize_title(&rc.name);
            let clean_pack_name = rc.pack_name.replace("ALeP - ", "").replace(".English", "");
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

            let side = match rc.type_code.as_deref() {
                Some("hero")
                | Some("ally")
                | Some("attachment")
                | Some("event")
                | Some("player-side-quest")
                | Some("contract")
                | Some("treasure") => "player",
                Some("quest") | Some("campaign") | Some("nightmare-setup") | Some("setup") => {
                    "quest"
                }
                _ => "encounter",
            };

            if seen_cards.insert(normalized_id.clone()) {
                cards.push(Card {
                    id: normalized_id.clone(),
                    title: rc.name,
                    title_normalized: base_normalized,
                    side: Some(side.to_string()),
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
                });
            }
        }

        let ringsdb_cards = crate::games::lotrlcg::api::fetch_all_cards().await?;
        for rc in ringsdb_cards {
            let base_normalized = normalize_title(&rc.name);

            let mut clean_pack_name = rc.pack_name.replace(".English", "");
            let is_alep = clean_pack_name.starts_with("ALeP - ");
            if is_alep {
                clean_pack_name = clean_pack_name.replace("ALeP - ", "");
            }

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

            let side = match rc.type_code.as_deref() {
                Some("hero")
                | Some("ally")
                | Some("attachment")
                | Some("event")
                | Some("player-side-quest")
                | Some("contract")
                | Some("treasure") => "player",
                Some("quest") | Some("campaign") | Some("nightmare-setup") | Some("setup") => {
                    "quest"
                }
                _ => "encounter",
            };

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
                    side: Some(side.to_string()),
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
    use crate::games::lotrlcg::models::HobCard;

    fn hob(slug: &str, card_set: &str, rings_db_card_id: &str) -> HobCard {
        HobCard {
            title: slug.split('-').next().unwrap_or(slug).to_string(),
            slug: slug.to_string(),
            card_set: card_set.to_string(),
            number: 0,
            quantity: Some(1),
            front: None,
            card_type: "Hero".into(),
            rings_db_card_id: Some(rings_db_card_id.to_string()),
        }
    }

    #[test]
    fn ringsdb_pack_prefix_drops_the_card_number() {
        assert_eq!(ringsdb_pack_prefix(&hob("a", "s", "02095")), Some("02"));
        assert_eq!(ringsdb_pack_prefix(&hob("a", "s", "148020")), Some("148"));
        assert_eq!(ringsdb_pack_prefix(&hob("a", "s", "")), None);
    }

    #[test]
    fn promo_slugs_flags_cards_from_a_foreign_pack() {
        // The Siege of Annuminas' own cards are all 000NN. The two Gen Con promo
        // heroes keep the ids of the packs they were really printed in.
        let cards = vec![
            hob("Standard-Game-Mode-TSoA", "The Siege of Annuminas", "00001"),
            hob(
                "Rebuild-the-Defenses-TSoA",
                "The Siege of Annuminas",
                "00003",
            ),
            hob("Defend-the-City-TSoA", "The Siege of Annuminas", "00004"),
            hob("Lead-the-Sortie-TSoA", "The Siege of Annuminas", "00005"),
            hob("Faramir-TSoA", "The Siege of Annuminas", "06081"),
            hob("Boromir-TSoA", "The Siege of Annuminas", "02095"),
        ];

        let promos = promo_slugs(&cards);
        assert_eq!(promos.len(), 2);
        assert!(promos.contains("Faramir-TSoA"));
        assert!(promos.contains("Boromir-TSoA"));
    }

    #[test]
    fn promo_slugs_leaves_reprint_collections_alone() {
        // Hero expansions are collections of reprints with no dominant prefix.
        // Without the threshold the rule would strip most of the product.
        let cards = vec![
            hob("Boromir-DoG", "Defenders of Gondor", "01001"),
            hob("Faramir-Ally-DoG", "Defenders of Gondor", "02010"),
            hob("Faramir-Hero-DoG", "Defenders of Gondor", "06028"),
            hob("Mablung-DoG", "Defenders of Gondor", "13003"),
        ];

        assert!(promo_slugs(&cards).is_empty());
    }
}
