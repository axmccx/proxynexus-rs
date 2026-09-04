use crate::card_source::DecklistProvider;
#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
use crate::error::Result;
use crate::games::GameAdapterInfo;
use crate::games::ahlcg::api::fetch_decklist_from_arkhamdb;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::ahlcg::api::{fetch_all_cards, fetch_packs};
#[cfg(not(target_arch = "wasm32"))]
use crate::games::ahlcg::models::AhdbCard;
use crate::models::Decklist;
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::{HashMap, HashSet};

pub struct AhlcgAdapter {}

impl Default for AhlcgAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AhlcgAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for AhlcgAdapter {
    fn game_id(&self) -> &'static str {
        "ahlcg"
    }

    fn game_name(&self) -> &'static str {
        "Arkham Horror (Chapter 1)"
    }

    fn subdomains(&self) -> Vec<&'static str> {
        vec!["ahlcg"]
    }
}

// Which generic card back a card needs, classified by `type_code` rather
// than `faction_code`/side
#[cfg(not(target_arch = "wasm32"))]
const PLAYER_TYPES: &[&str] = &["investigator", "asset", "event", "skill"];
#[cfg(not(target_arch = "wasm32"))]
const ENCOUNTER_TYPES: &[&str] = &[
    "enemy",
    "enemy_location",
    "treachery",
    "agenda",
    "act",
    "location",
    "scenario",
    "story",
    "key",
];

#[cfg(not(target_arch = "wasm32"))]
fn back_group_for(type_code: &str, subtype_code: Option<&str>) -> Option<String> {
    if matches!(subtype_code, Some("weakness") | Some("basicweakness")) {
        return Some("player".to_string());
    }
    if PLAYER_TYPES.contains(&type_code) {
        Some("player".to_string())
    } else if ENCOUNTER_TYPES.contains(&type_code) {
        Some("encounter".to_string())
    } else {
        None
    }
}

/// The name ArkhamDB prints on the card. `name` alone is the base name, shared
/// by every level of an upgradeable card, so the level has to be spelled out
/// for the title to name one card.
#[cfg(not(target_arch = "wasm32"))]
fn display_title(name: &str, xp: Option<i64>) -> String {
    match xp {
        Some(xp) if xp > 0 => format!("{} ({})", name, xp),
        _ => name.to_string(),
    }
}

/// Groups the codes that are one card, and labels each group with its lowest
/// code. Only reprints are grouped: `duplicated_by` is ArkhamDB saying two
/// codes are the same card printed twice.
#[cfg(not(target_arch = "wasm32"))]
fn same_card_groups(ahdb_cards: &[AhdbCard]) -> HashMap<String, String> {
    let mut adjacent: HashMap<&str, Vec<&str>> = HashMap::new();
    for card in ahdb_cards {
        for reprint in &card.duplicated_by {
            adjacent.entry(&card.code).or_default().push(reprint);
            adjacent.entry(reprint).or_default().push(&card.code);
        }
    }

    let mut group_of = HashMap::new();
    for card in ahdb_cards {
        if group_of.contains_key(&card.code) {
            continue;
        }

        let mut group = Vec::new();
        let mut seen = HashSet::from([card.code.as_str()]);
        let mut pending = vec![card.code.as_str()];
        while let Some(code) = pending.pop() {
            group.push(code);
            for next in adjacent.get(code).into_iter().flatten() {
                if seen.insert(next) {
                    pending.push(next);
                }
            }
        }

        let label = group.iter().min().expect("the card itself is in its group");
        for code in &group {
            group_of.insert((*code).to_string(), (*label).to_string());
        }
    }

    group_of
}

#[cfg(not(target_arch = "wasm32"))]
fn group_of<'a>(groups: &'a HashMap<String, String>, code: &'a str) -> &'a str {
    groups.get(code).map_or(code, String::as_str)
}

/// One title per code, spelling apart the titles that more than one card
/// answers to. Everything downstream reads the title as the card's name -- the
/// variant picker offers one title's printings as interchangeable, a typed card
/// list resolves a title to a single card -- so a title two cards share makes
/// them one card to the rest of the program.
///
/// Reprints are left sharing a title, because they are one card. Where a
/// subtitle does not do the separating, the lowest code keeps the plain title
/// and the others are spelled out against it, so that one parallel investigator
/// does not rename the ordinary one.
#[cfg(not(target_arch = "wasm32"))]
fn card_titles(ahdb_cards: &[AhdbCard]) -> HashMap<String, String> {
    let groups = same_card_groups(ahdb_cards);
    let base = |card: &AhdbCard| display_title(&card.name, card.xp);

    let mut sharing: HashMap<String, Vec<&AhdbCard>> = HashMap::new();
    for card in ahdb_cards {
        sharing
            .entry(normalize_title(&base(card)))
            .or_default()
            .push(card);
    }

    let mut titles = HashMap::with_capacity(ahdb_cards.len());
    for group in sharing.into_values() {
        let cards_sharing: HashSet<&str> = group
            .iter()
            .map(|card| group_of(&groups, &card.code))
            .collect();

        if cards_sharing.len() < 2 {
            for card in group {
                titles.insert(card.code.clone(), base(card));
            }
            continue;
        }

        // The subtitle names the card where every one of them carries a
        // subtitle and no two carry the same.
        let mut subnames: HashMap<&str, &str> = HashMap::new();
        for card in &group {
            subnames.insert(
                group_of(&groups, &card.code),
                card.subname.as_deref().unwrap_or_default(),
            );
        }
        let by_subname = subnames.values().all(|subname| !subname.is_empty())
            && subnames.values().collect::<HashSet<_>>().len() == cards_sharing.len();

        let plain = cards_sharing
            .iter()
            .min()
            .expect("a shared title has at least two cards");

        for card in &group {
            if !by_subname && group_of(&groups, &card.code) == *plain {
                titles.insert(card.code.clone(), base(card));
                continue;
            }

            // Two faces of one printing sit at the same position, and only the
            // code tells those apart.
            let shares_a_position = group.iter().any(|other| {
                group_of(&groups, &other.code) != group_of(&groups, &card.code)
                    && other.pack_code == card.pack_code
                    && other.position == card.position
            });

            let suffix = match (by_subname, shares_a_position) {
                (true, _) => card.subname.clone().unwrap_or_default(),
                (false, true) => card.code.clone(),
                (false, false) => format!("{} {}", card.pack_code, card.position),
            };
            titles.insert(card.code.clone(), format!("{} ({})", base(card), suffix));
        }
    }

    titles
}

/// ArkhamDB keeps both sides of a double-sided card under one `code` -- both
/// the ordinary flip case and the case where the back is a mechanically distinct card
#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(
    ahdb_cards: Vec<crate::games::ahlcg::models::AhdbCard>,
) -> (Vec<Card>, Vec<CardVersion>) {
    let titles = card_titles(&ahdb_cards);
    let mut cards = Vec::with_capacity(ahdb_cards.len());
    let mut card_versions = Vec::with_capacity(ahdb_cards.len());

    for card in ahdb_cards {
        let title = titles
            .get(&card.code)
            .cloned()
            .unwrap_or_else(|| display_title(&card.name, card.xp));

        cards.push(Card {
            id: card.code.clone(),
            title_normalized: normalize_title(&title),
            title,
            back_group: back_group_for(&card.type_code, card.subtype_code.as_deref()),
        });

        card_versions.push(CardVersion {
            card_id: card.code,
            pack_id: card.pack_code,
            quantity: card.quantity.unwrap_or(1),
            position: Some(card.position),
            api_id: None,
        });
    }

    (cards, card_versions)
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for AhlcgAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        let (ahdb_packs, ahdb_cards) = (fetch_packs().await?, fetch_all_cards().await?);

        let packs: Vec<Pack> = ahdb_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: pack.available,
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(ahdb_cards);

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
impl DecklistProvider for AhlcgAdapter {
    async fn fetch(&self, url: &str) -> Result<Decklist> {
        fetch_decklist_from_arkhamdb(url).await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::games::ahlcg::models::AhdbCard;

    fn card(code: &str, name: &str, type_code: &str, subtype_code: Option<&str>) -> AhdbCard {
        AhdbCard {
            code: code.to_string(),
            name: name.to_string(),
            pack_code: "core".to_string(),
            position: 1,
            type_code: type_code.to_string(),
            faction_code: "neutral".to_string(),
            quantity: Some(1),
            subtype_code: subtype_code.map(|s| s.to_string()),
            hidden: false,
            xp: None,
            subname: None,
            duplicated_by: Vec::new(),
        }
    }

    fn location(
        code: &str,
        name: &str,
        subname: Option<&str>,
        pack_code: &str,
        position: i64,
    ) -> AhdbCard {
        AhdbCard {
            pack_code: pack_code.to_string(),
            position,
            subname: subname.map(|s| s.to_string()),
            ..card(code, name, "location", None)
        }
    }

    fn player_card(code: &str, name: &str, pack_code: &str, position: i64, xp: i64) -> AhdbCard {
        AhdbCard {
            pack_code: pack_code.to_string(),
            position,
            xp: Some(xp),
            ..card(code, name, "skill", None)
        }
    }

    #[test]
    fn player_and_encounter_cards_get_the_correct_back_group() {
        let raw = vec![
            card("01006", "Roland Banks", "investigator", None),
            card("01121", "Ghoul Priest", "enemy", None),
        ];
        let (cards, versions) = build_cards_and_versions(raw);

        assert_eq!(cards.len(), 2);
        assert_eq!(versions.len(), 2);
        assert_eq!(cards[0].back_group.as_deref(), Some("player"));
        assert_eq!(cards[1].back_group.as_deref(), Some("encounter"));
    }

    #[test]
    fn a_weakness_carries_the_player_back_even_with_an_encounter_type_code() {
        // Mob Goons (08003) is type_code "enemy" but subtype "weakness": it
        // lives in the investigator's deck, so it prints with the player back.
        let raw = vec![
            card("08003", "Mob Goons", "enemy", Some("weakness")),
            card("01015", "Amnesia", "treachery", Some("basicweakness")),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].back_group.as_deref(), Some("player"));
        assert_eq!(cards[1].back_group.as_deref(), Some("player"));
    }

    #[test]
    fn an_unclassified_type_code_gets_no_back_group() {
        let raw = vec![card("00000", "Mystery", "investigator_choice", None)];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].back_group, None);
    }

    #[test]
    fn an_upgrade_is_a_card_of_its_own_title() {
        // Both are named "Deduction" in the API; only the level tells them
        // apart, and the pipeline groups by title.
        let raw = vec![
            player_card("01039", "Deduction", "core", 39, 0),
            player_card("02150", "Deduction", "tece", 150, 2),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].title, "Deduction");
        assert_eq!(cards[0].title_normalized, "deduction");
        assert_eq!(cards[1].title, "Deduction (2)");
        assert_ne!(cards[1].title_normalized, cards[0].title_normalized);
    }

    #[test]
    fn a_title_only_one_card_answers_to_is_left_alone() {
        let raw = vec![
            card("01001", "Roland Banks", "investigator", None),
            card("01121", "Ghoul Priest", "enemy", None),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].title, "Roland Banks");
        assert_eq!(cards[1].title, "Ghoul Priest");
    }

    #[test]
    fn reprints_keep_one_title_between_them() {
        // `60227` Seeking Answers (2) and its reprint `01685`. The two are
        // worded differently on the card, but ArkhamDB calls them one card and
        // the printing picker should offer them as one card's two printings.
        let mut original = player_card("60227", "Seeking Answers", "har", 27, 2);
        original.duplicated_by = vec!["01685".to_string()];
        let raw = vec![
            original,
            player_card("01685", "Seeking Answers", "rcore", 685, 2),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].title, "Seeking Answers (2)");
        assert_eq!(cards[1].title, "Seeking Answers (2)");
        assert_eq!(cards[0].title_normalized, cards[1].title_normalized);
    }

    #[test]
    fn two_cards_of_one_name_are_told_apart_by_their_subtitles() {
        // `03298` and `03299` Abbey Tower, a designed pair.
        let raw = vec![
            location("03298", "Abbey Tower", Some("The Path is Open"), "bsr", 298),
            location("03299", "Abbey Tower", Some("Spires Forbidden"), "bsr", 299),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].title, "Abbey Tower (The Path is Open)");
        assert_eq!(cards[1].title, "Abbey Tower (Spires Forbidden)");
    }

    #[test]
    fn a_subtitle_two_cards_share_does_not_separate_them() {
        // Two Historical Society · Record Office, one per scenario.
        let raw = vec![
            location(
                "03129",
                "Historical Society",
                Some("Record Office"),
                "eotp",
                129,
            ),
            location(
                "03138",
                "Historical Society",
                Some("Record Office"),
                "eotp",
                138,
            ),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].title, "Historical Society");
        assert_eq!(cards[1].title, "Historical Society (eotp 138)");
    }

    #[test]
    fn a_parallel_investigator_does_not_rename_the_ordinary_one() {
        // `90024` is a different Roland with the same name and subtitle. The
        // plain title stays with the card players mean by it.
        let mut roland = card("01001", "Roland Banks", "investigator", None);
        roland.subname = Some("The Fed".to_string());
        roland.duplicated_by = vec!["01501".to_string()];
        let mut revised = card("01501", "Roland Banks", "investigator", None);
        revised.subname = Some("The Fed".to_string());
        revised.pack_code = "rcore".to_string();
        let mut parallel = card("90024", "Roland Banks", "investigator", None);
        parallel.subname = Some("The Fed".to_string());
        parallel.pack_code = "btb".to_string();
        parallel.position = 24;

        let (cards, _) = build_cards_and_versions(vec![roland, revised, parallel]);

        assert_eq!(cards[0].title, "Roland Banks");
        assert_eq!(cards[1].title, "Roland Banks");
        assert_eq!(cards[2].title, "Roland Banks (btb 24)");
    }

    #[test]
    fn two_faces_at_one_position_fall_back_to_the_code() {
        // `09748a` and `09748b` Alien Frontier share a pack and a position, so
        // nothing but the code tells them apart.
        let raw = vec![
            location("09748a", "Alien Frontier", None, "tskc", 748),
            location("09748b", "Alien Frontier", None, "tskc", 748),
        ];
        let (cards, _) = build_cards_and_versions(raw);

        assert_eq!(cards[0].title, "Alien Frontier");
        assert_eq!(cards[1].title, "Alien Frontier (09748b)");
    }
}
