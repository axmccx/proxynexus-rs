use crate::error::Result;
use crate::games::fetch_json;
use crate::games::marvel_champions::models::{McdbCard, McdbPack};

const BASE_URL: &str = "https://marvelcdb.com/api/public";

pub async fn fetch_packs() -> Result<Vec<McdbPack>> {
    fetch_json(&format!("{BASE_URL}/packs/")).await
}

/// Every card, in one request.
///
/// Without `encounter=1` MarvelCDB returns the player cards alone.
pub async fn fetch_all_cards() -> Result<Vec<McdbCard>> {
    fetch_json(&format!("{BASE_URL}/cards/?encounter=1")).await
}
