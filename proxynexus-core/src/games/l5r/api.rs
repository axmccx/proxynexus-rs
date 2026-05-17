use crate::error::{ProxyNexusError, Result};
use crate::games::l5r::models::{Card, EmeraldDbDecklist, Pack};
use crate::models::{Decklist, DecklistEntry};
use serde::de::DeserializeOwned;

const CARDS_URL: &str = "https://www.emeralddb.org/api/cards";
const PACKS_URL: &str = "https://www.emeralddb.org/api/packs";
const DECKLISTS_URL: &str = "https://www.emeralddb.org/api/decklists";

pub async fn fetch_cards() -> Result<Vec<Card>> {
    fetch_json(CARDS_URL).await
}

pub async fn fetch_packs() -> Result<Vec<Pack>> {
    fetch_json(PACKS_URL).await
}

pub async fn fetch_decklist_from_emeralddb(url: &str) -> Result<Decklist> {
    let decklist_id = parse_emeralddb_url(url)?;
    let api_url = format!("{}/{}", DECKLISTS_URL, decklist_id);
    let decklist: EmeraldDbDecklist = fetch_json(&api_url).await?;

    let cards = decklist
        .cards
        .into_iter()
        .map(|(card_id, quantity)| DecklistEntry {
            card_id,
            pack_id: None,
            quantity,
        })
        .collect();

    Ok(Decklist { cards })
}

fn parse_emeralddb_url(url: &str) -> Result<String> {
    url.split("/decklists/")
        .nth(1)
        .map(|s| {
            s.trim_end_matches('/')
                .split('/')
                .next()
                .unwrap_or("")
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProxyNexusError::Internal("Invalid EmeraldDB decklist URL".into()))
}

async fn fetch_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let http_response = reqwest::get(url).await?;

        if !http_response.status().is_success() {
            return Err(ProxyNexusError::Internal(format!(
                "EmeraldDB returned error: {}",
                http_response.status()
            )));
        }

        Ok(http_response.json().await?)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let http_response = gloo_net::http::Request::get(url).send().await?;

        if !http_response.ok() {
            return Err(ProxyNexusError::Internal(format!(
                "EmeraldDB returned error: {}",
                http_response.status()
            )));
        }

        Ok(http_response.json().await?)
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn parses_simple_decklist_url() {
        let id = parse_emeralddb_url(
            "https://www.emeralddb.org/decklists/75ffc2ba-93a2-4551-bab3-2bb12ce015d7",
        )
        .unwrap();
        assert_eq!(id, "75ffc2ba-93a2-4551-bab3-2bb12ce015d7");
    }

    #[test]
    fn parses_decklist_url_with_trailing_slash() {
        let id = parse_emeralddb_url("https://www.emeralddb.org/decklists/abc123/").unwrap();
        assert_eq!(id, "abc123");
    }

    #[test]
    fn rejects_non_decklist_url() {
        assert!(parse_emeralddb_url("https://www.emeralddb.org/cards/foo").is_err());
    }
}
