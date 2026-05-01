use crate::error::{ProxyNexusError, Result};
use crate::games::netrunner::models::{
    NrdbCard, NrdbCardSet, NrdbPrinting, NrdbResponse, NrdbV2DeckResponse,
};
use crate::models::Decklist;
use std::collections::HashMap;

const BASE_URL: &str = "https://api.netrunnerdb.com/api/v3/public";

pub async fn fetch_card_sets() -> Result<Vec<NrdbCardSet>> {
    fetch_v3_endpoint(&format!("{}/card_sets?page[size]=1000", BASE_URL)).await
}

pub async fn fetch_cards() -> Result<Vec<NrdbCard>> {
    fetch_v3_endpoint(&format!("{}/cards?page[size]=1000", BASE_URL)).await
}

pub async fn fetch_printings() -> Result<Vec<NrdbPrinting>> {
    fetch_v3_endpoint(&format!("{}/printings?page[size]=1000", BASE_URL)).await
}

pub async fn fetch_v3_endpoint<T: for<'de> serde::Deserialize<'de>>(url: &str) -> Result<Vec<T>> {
    let mut all_data = Vec::new();
    let mut current_url = Some(url.to_string());

    while let Some(u) = current_url {
        #[cfg(not(target_arch = "wasm32"))]
        let json_str = reqwest::get(&u).await?.text().await?;

        #[cfg(target_arch = "wasm32")]
        let json_str = gloo_net::http::Request::get(&u)
            .send()
            .await?
            .text()
            .await?;

        let response: NrdbResponse<T> = serde_json::from_str(&json_str)?;
        all_data.extend(response.data);

        current_url = response.links.and_then(|l| l.next);
    }

    Ok(all_data)
}

pub async fn fetch_decklist_from_nrdb(url: &str) -> Result<Decklist> {
    let (deck_id, api_path) = parse_nrdb_url(url)?;

    let api_url = format!(
        "https://netrunnerdb.com/api/2.0/public/{}/{}",
        api_path, deck_id
    );

    let response: NrdbV2DeckResponse = {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let http_response = reqwest::get(&api_url).await?;

            if !http_response.status().is_success() {
                return Err(ProxyNexusError::Internal(format!(
                    "NetrunnerDB returned error: {}",
                    http_response.status()
                )));
            }

            http_response.json().await?
        }

        #[cfg(target_arch = "wasm32")]
        {
            let http_response = gloo_net::http::Request::get(&api_url).send().await?;

            if !http_response.ok() {
                return Err(ProxyNexusError::Internal(format!(
                    "NetrunnerDB returned error: {}",
                    http_response.status()
                )));
            }

            http_response.json().await?
        }
    };

    let cards_res = response
        .data
        .into_iter()
        .next()
        .ok_or_else(|| ProxyNexusError::Internal("Empty response from NetrunnerDB".into()))?
        .cards;

    let printing_codes: Vec<String> = cards_res.keys().cloned().collect();
    let filter_str = printing_codes.join(",");
    let printings_url = format!("{}/printings?filter[id]={}", BASE_URL, filter_str);

    let v3_printings: Vec<NrdbPrinting> = fetch_v3_endpoint(&printings_url).await?;

    let mut cards = HashMap::new();
    for printing in v3_printings {
        if let Some(&quantity) = cards_res.get(&printing.id) {
            cards.insert(printing.attributes.card_id, quantity);
        }
    }

    Ok(Decklist { cards })
}

fn parse_nrdb_url(url: &str) -> Result<(String, String)> {
    if url.contains("/decklist/") {
        let deck_id = url
            .split("/decklist/")
            .nth(1)
            .ok_or_else(|| ProxyNexusError::Internal("Invalid decklist URL".into()))?
            .split('/')
            .next()
            .ok_or_else(|| ProxyNexusError::Internal("Invalid decklist URL".into()))?
            .to_string();
        Ok((deck_id, "decklist".to_string()))
    } else if url.contains("/deck/view/") {
        let deck_id = url
            .split("/deck/view/")
            .nth(1)
            .ok_or_else(|| ProxyNexusError::Internal("Invalid deck URL".into()))?
            .trim_end_matches('/')
            .to_string();
        Ok((deck_id, "deck".to_string()))
    } else {
        Err(ProxyNexusError::Internal(
            "URL must be a NetrunnerDB decklist or deck URL".into(),
        ))
    }
}
