#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
#[cfg(not(target_arch = "wasm32"))]
use crate::catalog::{Card, CardVersion, Catalog, CatalogProvider, Pack};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
#[cfg(not(target_arch = "wasm32"))]
use crate::games::swlcg::models::{SwlcgCard, SwlcgPack};
use crate::games::GameAdapterInfo;
#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;

pub struct SwlcgAdapter {}

impl Default for SwlcgAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SwlcgAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

impl GameAdapterInfo for SwlcgAdapter {
    fn game_id(&self) -> &'static str {
        "swlcg"
    }

    fn game_name(&self) -> &'static str {
        "Star Wars LCG"
    }
}

// Which generic card back a card needs, classified by `side`.
// Balance of the Force packs have different backs than other packs,
// so we need to check for that as well.
#[cfg(not(target_arch = "wasm32"))]
const LIGHT_BACK: &str = "light";
#[cfg(not(target_arch = "wasm32"))]
const DARK_BACK: &str = "dark";
#[cfg(not(target_arch = "wasm32"))]
const BOTF_LIGHT_BACK: &str = "botf-light";
#[cfg(not(target_arch = "wasm32"))]
const BOTF_DARK_BACK: &str = "botf-dark";

#[cfg(not(target_arch = "wasm32"))]
fn back_group_for(
    pack_code: &str,
    side: &str,
    objective_set_number: Option<i64>,
) -> Option<String> {
    let is_botf_pack = pack_code == "balance-of-the-force" && objective_set_number.is_none();
    match (is_botf_pack, side) {
        (true, "Light") => Some(BOTF_LIGHT_BACK.to_string()),
        (true, "Dark") => Some(BOTF_DARK_BACK.to_string()),
        (false, "Light") => Some(LIGHT_BACK.to_string()),
        (false, "Dark") => Some(DARK_BACK.to_string()),
        _ => None,
    }
}

/// Turns the flat `swlcg_cards.json` card list into catalog rows.
#[cfg(not(target_arch = "wasm32"))]
fn build_cards_and_versions(
    swlcg_cards: Vec<crate::games::swlcg::models::SwlcgCard>,
) -> (Vec<Card>, Vec<CardVersion>) {
    let mut cards = Vec::with_capacity(swlcg_cards.len());
    let mut card_versions = Vec::with_capacity(swlcg_cards.len());

    for card in swlcg_cards {
        cards.push(Card {
            id: card.unique_id.clone(),
            title: card.label.clone(),
            title_normalized: normalize_title(&card.label),
            back_group: back_group_for(&card.pack_code, &card.side, card.objective_set_number),
        });

        let position = card.unique_id.parse::<i64>().ok();
        card_versions.push(CardVersion {
            card_id: card.unique_id,
            pack_id: card.pack_code,
            quantity: 1, // Cards are always individually listed in the JSON, so quantity is always 1.
            position,
            api_id: None,
        });
    }

    (cards, card_versions)
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl CatalogProvider for SwlcgAdapter {
    async fn fetch_catalog(&self) -> Result<Catalog> {
        // Load all packs (sets/expansions). All data is stored in a
        // single JSON file.
        let swlcg_packs: Vec<SwlcgPack> = serde_json::from_str(include_str!("swlcg_packs.json"))?;

        // Load every card across every pack. `swlcg_cards.json` is one bulk
        // file covering the whole catalog.
        let swlcg_cards: Vec<SwlcgCard> = serde_json::from_str(include_str!("swlcg_cards.json"))?;

        let packs: Vec<Pack> = swlcg_packs
            .into_iter()
            .map(|pack| Pack {
                id: pack.code,
                name: pack.name,
                date_release: Some(format!("{:02}", pack.position)),
            })
            .collect();

        let (cards, card_versions) = build_cards_and_versions(swlcg_cards);

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
    use crate::games::swlcg::models::SwlcgCard;

    fn card(
        unique_id: &str,
        name: &str,
        label: &str,
        pack_code: &str,
        card_type: &str,
        side: &str,
        affiliation: &str,
        objective_set_number: Option<i64>,
        objective_set_sequence: Option<i64>,
    ) -> SwlcgCard {
        SwlcgCard {
            unique_id: unique_id.to_string(),
            name: name.to_string(),
            label: label.to_string(),
            pack_code: pack_code.to_string(),
            card_type: card_type.to_string(),
            side: side.to_string(),
            affiliation: affiliation.to_string(),
            objective_set_number,
            objective_set_sequence,
        }
    }

    #[test]
    fn maps_unique_id_and_pack_code_onto_card_and_version() {
        // 0235, in the real catalog, is "Force Lightning" from
        // the Core Set (pack_code core-set) -- label == name
        // here since that title is globally unique in the real data.
        let (cards, versions) = build_cards_and_versions(vec![card(
            "0235",
            "Force Lightning",
            "Force Lightning",
            "core-set",
            "Event",
            "Dark",
            "Sith",
            Some(23),
            Some(5),
        )]);
        assert_eq!(cards.len(), 1);
        assert_eq!(versions.len(), 1);
        assert_eq!(cards[0].id, "0235");
        assert_eq!(cards[0].title, "Force Lightning");
        assert_eq!(cards[0].back_group.as_deref(), Some("dark"));
        assert_eq!(versions[0].card_id, "0235");
        assert_eq!(versions[0].pack_id, "core-set");
        assert_eq!(versions[0].position, Some(235));
    }

    #[test]
    fn card_quantity_is_carried_through_when_present() {
        let raw = card(
            "2426",
            "Tactical Planning",
            "Tactical Planning (242.6)",
            "meditation-and-mastery",
            "Fate",
            "Light",
            "Rebel Alliance",
            Some(242),
            Some(6),
        );

        let (_, versions) = build_cards_and_versions(vec![raw]);

        // Quantity is always 1, since each card is listed individually in the JSON.
        assert_eq!(versions[0].quantity, 1);
    }

    #[test]
    fn card_number_is_carried_through_to_version_position() {
        let raw = card(
            "2042",
            "Sith Wyrm",
            "Sith Wyrm (204.2)",
            "so-be-it",
            "Unit",
            "Dark",
            "Sith",
            Some(204),
            Some(2),
        );

        let (_, versions) = build_cards_and_versions(vec![raw]);

        assert_eq!(versions[0].position, Some(2042));
    }
}
