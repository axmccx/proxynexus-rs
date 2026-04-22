use crate::error::{ProxyNexusError, Result};
use crate::games::l5r::models::{Card, Pack};
use serde::de::DeserializeOwned;

const CARDS_URL: &str = "https://www.emeralddb.org/api/cards";
const PACKS_URL: &str = "https://www.emeralddb.org/api/packs";

pub async fn fetch_cards() -> Result<Vec<Card>> {
    fetch_json(CARDS_URL).await
}

pub async fn fetch_packs() -> Result<Vec<Pack>> {
    fetch_json(PACKS_URL).await
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
