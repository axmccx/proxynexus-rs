#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::marvel_champions::api::{fetch_all_cards, fetch_packs};
#[cfg(not(target_arch = "wasm32"))]
use crate::games::marvel_champions::models::McdbCard;
#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct MarvelChampionsAdapter {}

impl Default for MarvelChampionsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MarvelChampionsAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for MarvelChampionsAdapter {
    fn game_id(&self) -> &'static str {
        "marvel-champions"
    }

    fn game_name(&self) -> &'static str {
        "Marvel Champions"
    }
}

// Which generic card back a card needs, classified by `type_code`. Villains
// have their own back, separate from the rest of the encounter deck.
#[cfg(not(target_arch = "wasm32"))]
const PLAYER_TYPES: &[&str] = &[
    "hero",
    "alter_ego",
    "ally",
    "event",
    "upgrade",
    "support",
    "resource",
    "player_side_scheme",
];
#[cfg(not(target_arch = "wasm32"))]
const VILLAIN_TYPES: &[&str] = &["villain"];
#[cfg(not(target_arch = "wasm32"))]
const ENCOUNTER_TYPES: &[&str] = &[
    "minion",
    "main_scheme",
    "side_scheme",
    "attachment",
    "treachery",
    "obligation",
    "environment",
    "leader",
    "evidence_means",
    "evidence_motive",
    "evidence_opportunity",
];

#[cfg(not(target_arch = "wasm32"))]
fn back_group_for(type_code: &str) -> Option<String> {
    if PLAYER_TYPES.contains(&type_code) {
        Some("player".to_string())
    } else if VILLAIN_TYPES.contains(&type_code) {
        Some("villain".to_string())
    } else if ENCOUNTER_TYPES.contains(&type_code) {
        Some("encounter".to_string())
    } else {
        None
    }
}

/// The card a reprint reprints, or the card itself.
#[cfg(not(target_arch = "wasm32"))]
fn original<'a>(by_code: &HashMap<&str, &'a McdbCard>, card: &'a McdbCard) -> &'a McdbCard {
    let mut current = card;
    for _ in 0..by_code.len() {
        match current
            .duplicate_of_code
            .as_deref()
            .and_then(|code| by_code.get(code))
        {
            Some(next) => current = next,
            None => break,
        }
    }
    current
}

/// One title per code, spelling apart the titles that more than one card
/// answers to. A title two cards share makes them one card to the rest of the
/// program.
///
/// Reprints share their original's title. Within a shared title, a villain or
/// scheme stage names the card when no other card there has the same stage.
/// The lowest code without one keeps the plain title, and the rest are spelled
/// out by pack and position, or by code where two faces share a position.
#[cfg(not(target_arch = "wasm32"))]
fn card_titles(mcdb_cards: &[McdbCard]) -> HashMap<String, String> {
    let by_code: HashMap<&str, &McdbCard> = mcdb_cards
        .iter()
        .map(|card| (card.code.as_str(), card))
        .collect();

    let mut sharing: HashMap<String, BTreeMap<&str, &McdbCard>> = HashMap::new();
    for card in mcdb_cards {
        let first = original(&by_code, card);
        sharing
            .entry(normalize_title(&card.name))
            .or_default()
            .insert(first.code.as_str(), first);
    }

    let mut suffixes: HashMap<&str, String> = HashMap::new();
    for group in sharing.values().filter(|group| group.len() > 1) {
        let mut stages: HashMap<&str, usize> = HashMap::new();
        let mut positions: HashMap<(&str, i64), usize> = HashMap::new();
        for card in group.values() {
            if let Some(stage) = card.stage.as_deref() {
                *stages.entry(stage).or_default() += 1;
            }
            *positions
                .entry((card.pack_code.as_str(), card.position))
                .or_default() += 1;
        }

        let mut plain_taken = false;
        for (code, card) in group {
            let unique_stage = card.stage.as_deref().filter(|stage| stages[stage] == 1);
            let suffix = match unique_stage {
                Some(stage) => stage.to_string(),
                None if !plain_taken => {
                    plain_taken = true;
                    continue;
                }
                None if positions[&(card.pack_code.as_str(), card.position)] == 1 => {
                    format!("{} {}", card.pack_code, card.position)
                }
                None => code.to_string(),
            };
            suffixes.insert(code, suffix);
        }
    }

    mcdb_cards
        .iter()
        .map(|card| {
            let title = match suffixes.get(original(&by_code, card).code.as_str()) {
                Some(suffix) => format!("{} ({})", card.name, suffix),
                None => card.name.clone(),
            };
            (card.code.clone(), title)
        })
        .collect()
}

/// The cards that are printed as cards of their own.
///
/// A hidden card is the second face of the card linking to it, and its image is
/// that card's `~back`. SP//dr's Peni Parker is the exception: both of her
/// faces are hidden and no card links to her, so she is kept as the front.
///
/// MarvelCDB also lists most two-sided main schemes a second time under a plain
/// code (`01097` beside the linked `01097a` and `01097b`). The linked pair
/// describes both faces, so the plain code is dropped.
#[cfg(not(target_arch = "wasm32"))]
fn printed_cards(mcdb_cards: Vec<McdbCard>) -> Vec<McdbCard> {
    let backs: HashSet<String> = mcdb_cards
        .iter()
        .filter_map(|card| card.linked_to_code.clone())
        .collect();
    let paired: HashSet<String> = mcdb_cards
        .iter()
        .filter(|card| card.linked_to_code.is_some())
        .filter_map(|card| card.code.strip_suffix('a').map(str::to_string))
        .collect();

    mcdb_cards
        .into_iter()
        .filter(|card| {
            if card.hidden {
                card.linked_to_code.is_some() && !backs.contains(&card.code)
            } else {
                !paired.contains(&card.code)
            }
        })
        .collect()
}

/// A reprint is a version of the card it reprints rather than a card of its
/// own, so one image of the card covers every pack it is printed in. Each
/// version keeps its MarvelCDB code as its `api_id`, which is what a
/// collection's filenames name it by.
#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(mcdb_cards: Vec<McdbCard>) -> (Vec<Card>, Vec<CardVersion>) {
    let titles = card_titles(&mcdb_cards);
    let by_code: HashMap<&str, &McdbCard> = mcdb_cards
        .iter()
        .map(|card| (card.code.as_str(), card))
        .collect();
    let mut cards = Vec::with_capacity(mcdb_cards.len());
    let mut card_versions = Vec::with_capacity(mcdb_cards.len());

    for card in &mcdb_cards {
        let first = original(&by_code, card);
        if first.code == card.code {
            let title = titles
                .get(&card.code)
                .cloned()
                .unwrap_or_else(|| card.name.clone());

            cards.push(Card {
                id: card.code.clone(),
                title_normalized: normalize_title(&title),
                title,
                back_group: back_group_for(&card.type_code),
            });
        }

        card_versions.push(CardVersion {
            card_id: first.code.clone(),
            pack_id: card.pack_code.clone(),
            quantity: card.quantity.unwrap_or(1),
            position: Some(card.position),
            api_id: Some(card.code.clone()),
        });
    }

    (cards, card_versions)
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for MarvelChampionsAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let (mcdb_packs, mcdb_cards) = (fetch_packs().await?, fetch_all_cards().await?);

        let packs: Vec<Pack> = mcdb_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: pack.available,
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(printed_cards(mcdb_cards));

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

    fn card(code: &str, name: &str, type_code: &str) -> McdbCard {
        McdbCard {
            code: code.to_string(),
            name: name.to_string(),
            pack_code: "core".to_string(),
            position: 1,
            type_code: type_code.to_string(),
            hidden: false,
            linked_to_code: None,
            stage: None,
            duplicate_of_code: None,
            quantity: Some(1),
        }
    }

    fn at(code: &str, name: &str, type_code: &str, pack_code: &str, position: i64) -> McdbCard {
        McdbCard {
            pack_code: pack_code.to_string(),
            position,
            ..card(code, name, type_code)
        }
    }

    fn staged(code: &str, name: &str, stage: &str) -> McdbCard {
        McdbCard {
            stage: Some(stage.to_string()),
            ..card(code, name, "villain")
        }
    }

    fn title_of<'a>(cards: &'a [Card], id: &str) -> &'a str {
        &cards.iter().find(|card| card.id == id).unwrap().title
    }

    #[test]
    fn player_villain_and_encounter_cards_get_the_correct_back_group() {
        let raw = vec![
            card("01001a", "Spider-Man", "hero"),
            card("01094", "Rhino", "villain"),
            card("01140", "Ultron Drones", "minion"),
            card("26030", "Mystery", "unknown_type"),
        ];
        let (cards, versions) = build_cards_and_versions(raw);

        assert_eq!(versions.len(), 4);
        assert_eq!(cards[0].back_group.as_deref(), Some("player"));
        assert_eq!(cards[1].back_group.as_deref(), Some("villain"));
        assert_eq!(cards[2].back_group.as_deref(), Some("encounter"));
        assert_eq!(cards[3].back_group, None);
    }

    #[test]
    fn a_title_used_by_one_card_is_left_plain() {
        let (cards, _) = build_cards_and_versions(vec![card("01009", "Webbed Up", "attachment")]);

        assert_eq!(cards[0].title, "Webbed Up");
    }

    #[test]
    fn villain_stages_sharing_a_title_are_named_by_stage() {
        let raw = vec![
            at("27128", "Rhino", "minion", "sm", 128),
            staged("01094", "Rhino", "I"),
            staged("01095", "Rhino", "II"),
            staged("01096", "Rhino", "III"),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(title_of(&cards, "01094"), "Rhino (I)");
        assert_eq!(title_of(&cards, "01095"), "Rhino (II)");
        assert_eq!(title_of(&cards, "01096"), "Rhino (III)");
        assert_eq!(title_of(&cards, "27128"), "Rhino");
    }

    #[test]
    fn the_lowest_code_keeps_the_plain_title_and_the_rest_are_named_by_pack_and_position() {
        let raw = vec![
            at("04045", "Spider-Man", "ally", "trors", 45),
            at("01001a", "Spider-Man", "hero", "core", 1),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(title_of(&cards, "01001a"), "Spider-Man");
        assert_eq!(title_of(&cards, "04045"), "Spider-Man (trors 45)");
    }

    #[test]
    fn a_repeated_stage_falls_back_to_pack_and_position() {
        let raw = vec![
            McdbCard {
                stage: Some("I".to_string()),
                ..at("01134", "Ultron", "villain", "core", 134)
            },
            McdbCard {
                stage: Some("I".to_string()),
                ..at("45001", "Ultron", "villain", "aoa", 1)
            },
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(title_of(&cards, "01134"), "Ultron");
        assert_eq!(title_of(&cards, "45001"), "Ultron (aoa 1)");
    }

    #[test]
    fn faces_sharing_a_position_are_named_by_code() {
        let raw = vec![
            at("01043a", "Wakanda Forever!", "event", "core", 43),
            at("01043b", "Wakanda Forever!", "event", "core", 43),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(title_of(&cards, "01043a"), "Wakanda Forever!");
        assert_eq!(title_of(&cards, "01043b"), "Wakanda Forever! (01043b)");
    }

    fn linked(front: McdbCard, back: McdbCard) -> [McdbCard; 2] {
        [
            McdbCard {
                linked_to_code: Some(back.code.clone()),
                ..front
            },
            McdbCard {
                hidden: true,
                ..back
            },
        ]
    }

    fn codes(cards: &[McdbCard]) -> Vec<&str> {
        cards.iter().map(|card| card.code.as_str()).collect()
    }

    #[test]
    fn a_hidden_back_is_not_a_card_of_its_own() {
        let raw = linked(
            card("01001a", "Spider-Man", "hero"),
            card("01001b", "Peter Parker", "alter_ego"),
        )
        .to_vec();

        assert_eq!(codes(&printed_cards(raw)), ["01001a"]);
    }

    #[test]
    fn a_hidden_card_nothing_links_to_is_kept_as_the_front() {
        let [alter_ego, back] = linked(
            card("31002a", "Peni Parker", "alter_ego"),
            card("31002b", "SP//dr", "upgrade"),
        );
        let raw = vec![
            McdbCard {
                hidden: true,
                ..alter_ego
            },
            back,
        ];

        assert_eq!(codes(&printed_cards(raw)), ["31002a"]);
    }

    #[test]
    fn the_plain_code_beside_a_linked_pair_is_dropped() {
        let mut raw = vec![card("01097", "The Break-In!", "main_scheme")];
        raw.extend(linked(
            card("01097a", "The Break-In!", "main_scheme"),
            card("01097b", "The Break-In!", "main_scheme"),
        ));

        assert_eq!(codes(&printed_cards(raw)), ["01097a"]);
    }

    #[test]
    fn the_plain_code_beside_unlinked_copies_is_kept() {
        let raw = vec![
            card("01144", "Android Efficiency", "treachery"),
            card("01144a", "Android Efficiency", "treachery"),
            card("01144b", "Android Efficiency", "treachery"),
        ];

        assert_eq!(codes(&printed_cards(raw)), ["01144", "01144a", "01144b"]);
    }

    #[test]
    fn a_reprint_shares_its_originals_title() {
        let raw = vec![
            at("12020", "Swarm Tactics", "event", "wsp", 20),
            McdbCard {
                duplicate_of_code: Some("12020".to_string()),
                ..at("13020", "Swarm Tactics", "event", "qsv", 20)
            },
            at("40020", "Swarm Tactics", "event", "next_evol", 20),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(title_of(&cards, "12020"), "Swarm Tactics");
        assert_eq!(title_of(&cards, "40020"), "Swarm Tactics (next_evol 20)");
    }

    #[test]
    fn a_reprint_is_a_version_of_the_card_it_reprints() {
        let raw = vec![
            at("01088", "Energy", "resource", "core", 88),
            McdbCard {
                duplicate_of_code: Some("01088".to_string()),
                ..at("32022", "Energy", "resource", "mut_gen", 22)
            },
            McdbCard {
                duplicate_of_code: Some("01088".to_string()),
                ..at("32052", "Energy", "resource", "mut_gen", 52)
            },
            McdbCard {
                duplicate_of_code: Some("32022".to_string()),
                ..at("40061", "Energy", "resource", "next_evol", 61)
            },
        ];
        let (cards, versions) = build_cards_and_versions(raw);

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].id, "01088");
        let placed: Vec<(&str, &str, Option<i64>, Option<&str>)> = versions
            .iter()
            .map(|v| {
                (
                    v.card_id.as_str(),
                    v.pack_id.as_str(),
                    v.position,
                    v.api_id.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            placed,
            [
                ("01088", "core", Some(88), Some("01088")),
                ("01088", "mut_gen", Some(22), Some("32022")),
                ("01088", "mut_gen", Some(52), Some("32052")),
                ("01088", "next_evol", Some(61), Some("40061")),
            ]
        );
    }
}
