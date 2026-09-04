use crate::error::{ProxyNexusError, Result};
use crate::games::ahlcg::models::{AhdbCard, AhdbDecklist, AhdbPack};
use crate::games::fetch_json;
use crate::models::{Decklist, DecklistEntry};

const BASE_URL: &str = "https://arkhamdb.com/api/public";

pub async fn fetch_packs() -> Result<Vec<AhdbPack>> {
    fetch_json(&format!("{BASE_URL}/packs/")).await
}

/// Every card, in one request.
///
/// `encounter=1` is what makes it every card: without it ArkhamDB returns the
/// player cards alone, 1983 of the 5929 records, which reads like a truncated
/// response rather than a filtered one.
pub async fn fetch_all_cards() -> Result<Vec<AhdbCard>> {
    let cards: Vec<AhdbCard> = fetch_json(&format!("{BASE_URL}/cards/?encounter=1")).await?;

    // A card whose two faces are each a card in their own right is returned
    // twice, and the half ArkhamDB does not index the card under is flagged
    // hidden. That half is a face rather than a card, so it is dropped: keeping
    // it would put a second catalog entry behind one printed card.
    Ok(cards.into_iter().filter(|card| !card.hidden).collect())
}

pub async fn fetch_decklist_from_arkhamdb(url: &str) -> Result<Decklist> {
    let decklist_id = parse_arkhamdb_decklist_url(url)?;
    let api_url = format!("{BASE_URL}/decklist/{decklist_id}");

    let response: AhdbDecklist = fetch_json(&api_url).await?;

    let cards = response
        .slots
        .into_iter()
        .filter_map(|(card_id, quantity)| {
            u32::try_from(quantity).ok().map(|quantity| DecklistEntry {
                card_id,
                pack_id: None,
                quantity,
                position: None,
            })
        })
        .collect();

    Ok(Decklist { cards })
}

fn parse_arkhamdb_decklist_url(url: &str) -> Result<String> {
    url.split("/decklist/view/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|id| id.to_string())
        .ok_or_else(|| ProxyNexusError::Internal("URL must be an ArkhamDB decklist URL".into()))
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn parses_simple_decklist_url() {
        let id =
            parse_arkhamdb_decklist_url("https://arkhamdb.com/decklist/view/12345/some-deck-1.0")
                .unwrap();
        assert_eq!(id, "12345");
    }

    #[test]
    fn parses_decklist_url_with_trailing_slash() {
        let id = parse_arkhamdb_decklist_url("https://arkhamdb.com/decklist/view/12345/").unwrap();
        assert_eq!(id, "12345");
    }

    #[test]
    fn rejects_non_decklist_url() {
        assert!(parse_arkhamdb_decklist_url("https://arkhamdb.com/card/01001").is_err());
    }
}
