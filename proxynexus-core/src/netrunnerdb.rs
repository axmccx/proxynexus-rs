use crate::card_source::{CardSource, NrdbUrl};
use crate::models::CardRequest;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct NrdbResponse {
    data: Vec<NrdbDeck>,
}

#[derive(Debug, Deserialize)]
struct NrdbDeck {
    cards: HashMap<String, u32>,
}

impl CardSource for NrdbUrl {
    fn to_card_requests(&self) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        fetch_card_requests_from_nrdb_url(&self.0)
    }
}

fn fetch_card_requests_from_nrdb_url(
    url: &str,
) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
    let (deck_id, api_path) = parse_nrdb_url(url)?;

    let api_url = format!(
        "https://netrunnerdb.com/api/2.0/public/{}/{}",
        api_path, deck_id
    );

    let http_response = reqwest::blocking::get(&api_url)
        .map_err(|e| format!("Failed to connect to NetrunnerDB: {}", e))?;

    if !http_response.status().is_success() {
        return Err(format!("NetrunnerDB returned error: {}", http_response.status()).into());
    }

    let response: NrdbResponse = http_response
        .json()
        .map_err(|e| format!("Failed to parse NetrunnerDB response: {}", e))?;

    let cards = &response
        .data
        .get(0)
        .ok_or("Empty response from NetrunnerDB")?
        .cards;

    Ok(cards
        .into_iter()
        .flat_map(|(code, qty)| {
            std::iter::repeat(CardRequest {
                code: code.clone(),
                variant: None,
                collection: None,
            })
            .take(*qty as usize)
        })
        .collect())
}

fn parse_nrdb_url(url: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    if url.contains("/decklist/") {
        let deck_id = url
            .split("/decklist/")
            .nth(1)
            .ok_or("Invalid decklist URL")?
            .split('/')
            .next()
            .ok_or("Invalid decklist URL")?
            .to_string();
        Ok((deck_id, "decklist".to_string()))
    } else if url.contains("/deck/view/") {
        let deck_id = url
            .split("/deck/view/")
            .nth(1)
            .ok_or("Invalid deck URL")?
            .trim_end_matches('/')
            .to_string();
        Ok((deck_id, "deck".to_string()))
    } else {
        Err("URL must be a NetrunnerDB decklist or deck URL".into())
    }
}
