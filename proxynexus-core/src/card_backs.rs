use crate::error::{ProxyNexusError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardBack {
    pub back_group: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub asset_id: &'static str,
    pub has_bleed: bool,
}

include!(concat!(env!("OUT_DIR"), "/card_back_table.rs"));

const PREFERRED_LABEL: &str = "proxy";

pub fn card_backs(game_id: &str) -> &'static [CardBack] {
    CARD_BACKS
        .iter()
        .find(|(id, _)| *id == game_id)
        .map_or(&[], |(_, backs)| *backs)
}

pub fn back_labels(game_id: &str) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = card_backs(game_id).iter().map(|back| back.label).collect();
    labels.sort_unstable();
    labels.dedup();
    labels
}

pub fn default_label(game_id: &str) -> Option<&'static str> {
    let labels = back_labels(game_id);
    labels
        .iter()
        .copied()
        .find(|label| *label == PREFERRED_LABEL)
        .or_else(|| labels.first().copied())
}

pub fn card_back(
    game_id: &str,
    back_group: &str,
    label: Option<&str>,
) -> Option<&'static CardBack> {
    let label = label.or(default_label(game_id))?;
    card_backs(game_id)
        .iter()
        .find(|back| back.back_group == back_group && back.label == label)
}

pub async fn fetch_card_backs(game_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let backs = card_backs(game_id);
    let mut loaded = Vec::with_capacity(backs.len());
    for back in backs {
        loaded.push((back.file.to_string(), load_card_back(back.asset_id).await?));
    }
    Ok(loaded)
}

pub async fn load_card_back(asset_id: &str) -> Result<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        CARD_BACK_BYTES
            .iter()
            .find(|(id, _)| *id == asset_id)
            .map(|(_, bytes)| bytes.to_vec())
            .ok_or_else(|| {
                ProxyNexusError::Internal(format!("No card back is published as '{}'.", asset_id))
            })
    }

    #[cfg(target_arch = "wasm32")]
    {
        use gloo_net::http::Request;

        let url = format!("card_backs/{}", asset_id);
        let response = Request::get(&url).send().await?;

        if !response.ok() {
            return Err(ProxyNexusError::Internal(format!(
                "Failed to fetch {}: HTTP {}",
                url,
                response.status()
            )));
        }

        Ok(response.binary().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    #[test]
    fn an_asset_id_is_the_game_id_and_the_file_name() {
        for (game_id, backs) in CARD_BACKS {
            for back in *backs {
                assert_eq!(back.asset_id, format!("{}_{}", game_id, back.file));
                let bleed = if back.has_bleed { ".bleed" } else { "" };
                let extension = back.file.rsplit('.').next().unwrap();
                assert_eq!(
                    back.file,
                    format!("{}_{}{}.{}", back.back_group, back.label, bleed, extension)
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn every_published_back_has_bytes_behind_it() {
        for (_, backs) in CARD_BACKS {
            for back in *backs {
                let found = CARD_BACK_BYTES
                    .iter()
                    .find(|(asset_id, _)| *asset_id == back.asset_id);
                let (_, bytes) = found.unwrap_or_else(|| panic!("no bytes for {}", back.asset_id));
                assert!(!bytes.is_empty(), "{} is empty", back.asset_id);
            }
        }
    }

    #[test]
    fn every_back_group_of_a_game_offers_the_same_labels() {
        // Picking a label has to leave every card with a back, so a label that
        // only some groups carry would silently blank the rest.
        for (game_id, backs) in CARD_BACKS {
            let mut by_group: HashMap<&str, BTreeSet<&str>> = HashMap::new();
            for back in *backs {
                by_group
                    .entry(back.back_group)
                    .or_default()
                    .insert(back.label);
            }

            let mut groups = by_group.iter();
            let Some((first_group, expected)) = groups.next() else {
                continue;
            };
            for (group, labels) in groups {
                assert_eq!(
                    labels, expected,
                    "{}: back group '{}' offers {:?}, but '{}' offers {:?}",
                    game_id, group, labels, first_group, expected
                );
            }
        }
    }

    #[test]
    fn labels_are_listed_once_and_in_order() {
        for (game_id, _) in CARD_BACKS {
            let labels = back_labels(game_id);
            let mut expected = labels.clone();
            expected.sort_unstable();
            expected.dedup();
            assert_eq!(labels, expected, "{}", game_id);
        }
    }

    #[test]
    fn no_label_takes_proxy_then_the_first_alphabetically() {
        for (game_id, _) in CARD_BACKS {
            let labels = back_labels(game_id);
            let expected = if labels.contains(&PREFERRED_LABEL) {
                Some(PREFERRED_LABEL)
            } else {
                labels.first().copied()
            };
            assert_eq!(default_label(game_id), expected, "{}", game_id);
        }
    }

    #[test]
    fn every_shipped_back_is_found_by_its_own_group_and_label() {
        for (game_id, backs) in CARD_BACKS {
            for back in *backs {
                assert_eq!(
                    card_back(game_id, back.back_group, Some(back.label)),
                    Some(back),
                    "{} {} {}",
                    game_id,
                    back.back_group,
                    back.label
                );
            }
        }
    }

    #[test]
    fn an_unknown_game_group_or_label_has_no_back() {
        // A game covers some back groups and not others, and the ones it does
        // not cover print a blank reverse rather than the wrong art.
        assert!(card_backs("no-such-game").is_empty());
        assert_eq!(default_label("no-such-game"), None);
        assert_eq!(card_back("no-such-game", "corp", None), None);

        for (game_id, backs) in CARD_BACKS {
            assert_eq!(
                card_back(game_id, "no-such-group", None),
                None,
                "{}",
                game_id
            );
            if let Some(back) = backs.first() {
                assert_eq!(
                    card_back(game_id, back.back_group, Some("no-such-label")),
                    None,
                    "{}",
                    game_id
                );
            }
        }
    }
}
