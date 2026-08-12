pub mod agot;
pub mod l5r;
pub mod lotrlcg;
pub mod netrunner;
pub mod netrunner_reboot;
use crate::card_source::DecklistProvider;
use crate::error::{ProxyNexusError, Result};
use crate::games::agot::adapter::AgotAdapter;
use crate::games::l5r::adapter::L5rAdapter;
use crate::games::lotrlcg::adapter::LotrLcgAdapter;
use crate::games::netrunner::adapter::NetrunnerAdapter;
use crate::games::netrunner_reboot::adapter::NetrunnerRebootAdapter;
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
        _ => None,
    }
}

pub async fn fetch_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let domain = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()?;

        let http_response = client.get(url).send().await?;

        if !http_response.status().is_success() {
            return Err(ProxyNexusError::Internal(format!(
                "{} returned error: {}",
                domain,
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
                "{} returned error: {}",
                domain,
                http_response.status()
            )));
        }

        Ok(http_response.json().await?)
    }
}
