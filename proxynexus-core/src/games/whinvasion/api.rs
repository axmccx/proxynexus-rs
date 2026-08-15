use crate::error::{ProxyNexusError, Result};
use crate::games::fetch_json;
use crate::games::whinvasion::models::{WhiCard, WhiPack};
use crate::models::Decklist;

const BASE_URL: &str =
    "https://raw.githubusercontent.com/mcwookie/warhammer-invasion-lcg-lists/main";

/// Fetches all packs (sets/expansions). All data is stored in a single JSON
/// file, so this is a single request.
pub async fn fetch_packs() -> Result<Vec<WhiPack>> {
    fetch_json(&format!("{BASE_URL}/whi_packs.json")).await
}

/// Fetches every card across every pack. Unlike ArkhamDB/MarvelCDB, there's
/// no per-pack endpoint here -- `whi_full.json` is one bulk file covering
/// the whole catalog, so this is also a single request.
pub async fn fetch_all_cards() -> Result<Vec<WhiCard>> {
    fetch_json(&format!("{BASE_URL}/whi_full.json")).await
}

/// Fetches a decklist by its URL. Decklists are not currently supported for
/// Warhammer Invasion (no known public decklist source), so this always
/// returns an error. Not wired into a `DecklistProvider` impl yet -- kept as
/// a stub for parity with the other adapters, same as
/// `marvel_champions::api::fetch_decklist_from_marvelcdb`.
#[allow(dead_code)]
pub async fn fetch_decklist_from_url(_url: &str) -> Result<Decklist> {
    Err(ProxyNexusError::Internal(
        "Decklists are not currently supported for Warhammer Invasion".into(),
    ))
}
