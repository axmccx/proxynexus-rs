use crate::card_store::normalize_title;
use crate::error::{ProxyNexusError, Result};
use crate::games::fetch_json;
use crate::games::netrunner_reboot::models::{RebootCard, RebootDeck, RebootPack, RebootResponse};
use crate::models::{Decklist, DecklistEntry};
use serde::de::DeserializeOwned;

const BASE_URL: &str = "https://nrdb.reteki.fun/api/2.0/public";
const HOST: &str = "nrdb.reteki.fun";

pub async fn fetch_all_cards() -> Result<Vec<RebootCard>> {
    fetch_data(&format!("{}/cards", BASE_URL)).await
}

pub async fn fetch_all_packs() -> Result<Vec<RebootPack>> {
    fetch_data(&format!("{}/packs", BASE_URL)).await
}

async fn fetch_data<T: DeserializeOwned>(url: &str) -> Result<Vec<T>> {
    let response: RebootResponse<T> = fetch_json(url).await?;
    Ok(response.data)
}

pub async fn fetch_decklist_from_reteki(url: &str) -> Result<Decklist> {
    let (deck_id, api_path) = parse_reteki_url(url)?;

    let decks: Vec<RebootDeck> =
        fetch_data(&format!("{}/{}/{}", BASE_URL, api_path, deck_id)).await?;

    let deck = decks
        .into_iter()
        .next()
        .ok_or_else(|| ProxyNexusError::Internal("Empty response from reteki".into()))?;

    let all_cards = fetch_all_cards().await?;
    let mut code_to_card = std::collections::HashMap::new();
    for card in all_cards {
        code_to_card.insert(card.code.clone(), card);
    }

    let mut cards = Vec::new();
    for (code, quantity) in deck.cards {
        if let Some(card) = code_to_card.get(&code) {
            cards.push(DecklistEntry {
                card_id: normalize_title(&card.title),
                pack_id: Some(card.pack_code.clone()),
                quantity,
            });
        }
    }

    Ok(Decklist { cards })
}

fn parse_reteki_url(url: &str) -> Result<(String, String)> {
    if !url.contains(HOST) {
        return Err(ProxyNexusError::Internal(format!(
            "URL must be a {} decklist or deck URL",
            HOST
        )));
    }

    if let Some(deck_id) = extract_path_segment(url, "/decklist/") {
        Ok((deck_id, "decklist".to_string()))
    } else if let Some(deck_id) = extract_path_segment(url, "/deck/view/") {
        Ok((deck_id, "deck".to_string()))
    } else {
        Err(ProxyNexusError::Internal(format!(
            "URL must be a {} decklist or deck URL",
            HOST
        )))
    }
}

fn extract_path_segment(url: &str, segment: &str) -> Option<String> {
    url.split(segment)
        .nth(1)
        .map(|s| s.split(['/', '?', '#']).next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_decklist_url_with_a_title_slug() {
        assert_eq!(
            parse_reteki_url("https://nrdb.reteki.fun/en/decklist/422/get-carried-by-snowflake")
                .unwrap(),
            ("422".to_string(), "decklist".to_string())
        );
    }

    #[test]
    fn parses_a_deck_view_url() {
        assert_eq!(
            parse_reteki_url("https://nrdb.reteki.fun/en/deck/view/1234").unwrap(),
            ("1234".to_string(), "deck".to_string())
        );
    }

    #[test]
    fn rejects_a_netrunnerdb_url() {
        assert!(parse_reteki_url("https://netrunnerdb.com/en/decklist/422/some-deck").is_err());
    }
}
