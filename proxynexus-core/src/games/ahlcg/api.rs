use crate::error::{ProxyNexusError, Result};
use crate::games::ahlcg::models::{AhdbCard, AhdbDecklist, AhdbPack};
use crate::games::fetch_json;
use crate::models::{Decklist, DecklistEntry};
use std::collections::HashSet;

const BASE_URL: &str = "https://arkhamdb.com/api/public";

/// Fetches all packs (sets/expansions). ArkhamDB returns this as a plain
/// JSON array, unlike NetrunnerDB's paginated JSON:API envelope -- no
/// pagination loop needed.
pub async fn fetch_packs() -> Result<Vec<AhdbPack>> {
    fetch_json(&format!("{BASE_URL}/packs/")).await
}

/// Fetches every card, correctly and completely, by iterating pack-by-pack
/// via fetch_cards_for_pack() rather than the bulk /api/public/cards/
/// endpoint.
///
/// Confirmed by direct comparison that the bulk endpoint is badly
/// incomplete: it returned 1,983 cards total against packs.json's summed
/// `total` field of 8,422, and for the Core Set specifically returned only
/// 105 of the pack's 184 cards (183 via the per-pack endpoint). Same class
/// of issue as the MarvelCDB adapter's bulk-endpoint bug, just worse.
///
/// Slower (~115 requests instead of 1) but the resulting catalog is
/// actually complete.
pub async fn fetch_all_cards(packs: &[AhdbPack]) -> Result<Vec<AhdbCard>> {
    let mut cards = Vec::new();
    let mut seen_codes = HashSet::new();

    for pack in packs {
        let pack_cards = fetch_cards_for_pack(&pack.code).await?;
        for card in pack_cards {
            if seen_codes.insert(card.code.clone()) {
                cards.push(card);
            }
        }
    }

    Ok(cards)
}

/// Fetches cards for a single pack. Unlike MarvelCDB, ArkhamDB represents a
/// double-sided card as one entry with `imagesrc`/`backimagesrc` fields
/// rather than a separate hidden card, so no card-splitting is needed here
/// -- each entry becomes one `Card`/`CardVersion`, and its back image (be it
/// a plain flip side or a mechanically distinct card) resolves separately
/// via the `{card_id}@{pack_id}~back` filename convention.
pub async fn fetch_cards_for_pack(pack_code: &str) -> Result<Vec<AhdbCard>> {
    fetch_json(&format!("{BASE_URL}/cards/{pack_code}")).await
}

/// Fetches a decklist by its ArkhamDB URL.
///
/// Accepts URLs like:
///   https://arkhamdb.com/decklist/view/12345/some-deck-name-1.0
///
/// Does not add the investigator card itself, matching how every other
/// adapter's decklist parsing leaves the identity/investigator card for the
/// user to add separately -- ArkhamDB reports it via a separate
/// `investigator_code` field, not in `slots`.
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
