pub mod agot;
pub mod l5r;
pub mod lotrlcg;
pub mod netrunner;
pub mod netrunner_reboot;
pub mod whinvasion;
use crate::card_source::DecklistProvider;
use crate::error::{ProxyNexusError, Result};
use crate::games::agot::adapter::AgotAdapter;
use crate::games::l5r::adapter::L5rAdapter;
use crate::games::lotrlcg::adapter::LotrLcgAdapter;
use crate::games::netrunner::adapter::NetrunnerAdapter;
use crate::games::netrunner_reboot::adapter::NetrunnerRebootAdapter;
use crate::games::whinvasion::adapter::WhiAdapter;
use crate::mpc::CardBackProvider;
use serde::de::DeserializeOwned;

pub trait GameAdapterInfo {
    fn game_id(&self) -> &'static str;
    fn game_name(&self) -> &'static str;
    fn subdomains(&self) -> Vec<&'static str> {
        vec![]
    }
}

pub fn get_game_id_by_subdomain(subdomain: &str) -> Option<&'static str> {
    let adapters: Vec<Box<dyn GameAdapterInfo>> = vec![
        Box::new(NetrunnerAdapter::new()),
        Box::new(NetrunnerRebootAdapter::new()),
        Box::new(L5rAdapter::new()),
        Box::new(AgotAdapter::new()),
        Box::new(LotrLcgAdapter::new()),
        Box::new(WhiAdapter::new()),
    ];

    for adapter in adapters {
        if adapter.subdomains().contains(&subdomain) {
            return Some(adapter.game_id());
        }
    }
    None
}

pub fn get_decklist_adapter(game_id: &str) -> Option<Box<dyn DecklistProvider>> {
    match game_id {
        "netrunner" => Some(Box::new(NetrunnerAdapter::new())),
        "netrunner-reboot" => Some(Box::new(NetrunnerRebootAdapter::new())),
        "l5r" => Some(Box::new(L5rAdapter::new())),
        "agot" => Some(Box::new(AgotAdapter::new())),
        "lotrlcg" => Some(Box::new(LotrLcgAdapter::new())),
        _ => None,
    }
}

pub fn get_card_back_adapter(game_id: &str) -> Option<Box<dyn CardBackProvider>> {
    match game_id {
        "netrunner" => Some(Box::new(NetrunnerAdapter::new())),
        "netrunner-reboot" => Some(Box::new(NetrunnerRebootAdapter::new())),
        "l5r" => Some(Box::new(L5rAdapter::new())),
        "whinvasion" => Some(Box::new(WhiAdapter::new())),
        _ => None,
    }
}

const MAX_FETCH_ATTEMPTS: u32 = 4;
const FETCH_RETRY_BASE_DELAY_MS: u64 = 2_000;

#[cfg(not(target_arch = "wasm32"))]
const FETCH_TIMEOUT_SECS: u64 = 120;

async fn retry_delay(attempt: u32) {
    let ms = FETCH_RETRY_BASE_DELAY_MS * 2u64.pow(attempt - 1);

    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;

    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

pub async fn fetch_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let domain = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url);

    let mut attempt = 1;

    loop {
        let outcome = fetch_json_once::<T>(url, domain).await;

        let retryable = match &outcome {
            Ok(_) => false,
            Err(e) => e.is_retryable_fetch(),
        };

        if !retryable || attempt >= MAX_FETCH_ATTEMPTS {
            return outcome;
        }

        if let Err(e) = &outcome {
            tracing::warn!(
                "{} request failed (attempt {}/{}): {}. Retrying...",
                domain,
                attempt,
                MAX_FETCH_ATTEMPTS,
                e
            );
        }

        retry_delay(attempt).await;
        attempt += 1;
    }
}

async fn fetch_json_once<T: DeserializeOwned>(url: &str, domain: &str) -> Result<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
            .build()?;

        let http_response = client.get(url).send().await?;

        let status = http_response.status();
        if !status.is_success() {
            return Err(ProxyNexusError::HttpStatus {
                domain: domain.to_string(),
                status: status.as_u16(),
            });
        }

        Ok(http_response.json().await?)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let http_response = gloo_net::http::Request::get(url).send().await?;

        if !http_response.ok() {
            return Err(ProxyNexusError::HttpStatus {
                domain: domain.to_string(),
                status: http_response.status(),
            });
        }

        Ok(http_response.json().await?)
    }
}
