#[cfg(not(target_arch = "wasm32"))]
use crate::card_store::normalize_title;
use crate::error::{ProxyNexusError, Result};
use crate::games::agot::models::{AgotCard, AgotDecklist, AgotPack};
use crate::games::fetch_json;
use crate::models::{Decklist, DecklistEntry};

const BASE_URL: &str = "https://thronesdb.com/api/public";

pub async fn fetch_all_packs() -> Result<Vec<AgotPack>> {
    fetch_json(&format!("{}/packs/", BASE_URL)).await
}

pub async fn fetch_all_cards() -> Result<Vec<AgotCard>> {
    fetch_json(&format!("{}/cards/", BASE_URL)).await
}

pub async fn fetch_decklist_from_thronesdb(url: &str) -> Result<Decklist> {
    let deck_id = parse_thronesdb_url(url)?;
    let api_url = format!("{}/decklist/{}", BASE_URL, deck_id);
    let decklist_response: AgotDecklist = fetch_json(&api_url).await?;

    let all_cards = fetch_all_cards().await?;
    let mut code_to_card = std::collections::HashMap::new();
    for card in all_cards {
        code_to_card.insert(card.code.clone(), card);
    }

    let mut cards = Vec::new();
    for (code, quantity) in decklist_response.slots {
        if let Some(card) = code_to_card.get(&code) {
            #[cfg(not(target_arch = "wasm32"))]
            let card_id = normalize_title(&card.label);

            #[cfg(target_arch = "wasm32")]
            let card_id = crate::card_store::normalize_title(&card.label);

            cards.push(DecklistEntry {
                card_id,
                pack_id: Some(card.pack_code.clone()),
                quantity,
                position: None,
            });
        }
    }

    Ok(Decklist { cards })
}

fn parse_thronesdb_url(url: &str) -> Result<String> {
    extract_path_segment(url, "/decklist/view/")
        .or_else(|| extract_path_segment(url, "/api/public/decklist/"))
        .ok_or_else(|| ProxyNexusError::Internal("Invalid ThronesDB decklist URL".into()))
}

fn extract_path_segment(url: &str, segment: &str) -> Option<String> {
    url.split(segment)
        .nth(1)
        .map(|s| {
            s.trim_end_matches('/')
                .split(['/', '?', '#'])
                .next()
                .unwrap_or("")
                .to_string()
        })
        .filter(|s| !s.is_empty())
}
