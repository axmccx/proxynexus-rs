use crate::db_storage::DbStorage;
use crate::error::{ProxyNexusError, Result};
use crate::models::Printing;
use std::collections::BTreeSet;

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

pub async fn allowed_labels(
    db: &mut DbStorage,
    game_id: &str,
    printings: &[Printing],
) -> Result<Vec<&'static str>> {
    let restrictions = db.get_back_restrictions(game_id).await?;

    let restricted: BTreeSet<&str> = printings
        .iter()
        .filter_map(|printing| restrictions.get(&printing.collection))
        .flatten()
        .map(String::as_str)
        .collect();

    let mut labels: Vec<&'static str> = backs_of(game_id)
        .iter()
        .map(|back| back.label)
        .filter(|label| !restricted.contains(label))
        .collect();
    labels.sort_unstable();
    labels.dedup();
    Ok(labels)
}

pub fn default_label(labels: &[&'static str]) -> Option<&'static str> {
    labels
        .iter()
        .copied()
        .find(|label| *label == PREFERRED_LABEL)
        .or_else(|| labels.first().copied())
}

pub fn card_back(game_id: &str, back_group: &str, label: &str) -> Option<&'static CardBack> {
    backs_of(game_id)
        .iter()
        .find(|back| back.back_group == back_group && back.label == label)
}

pub async fn fetch_card_backs(
    game_id: &str,
    labels: &[&'static str],
) -> Result<Vec<(&'static CardBack, Vec<u8>)>> {
    let mut loaded = Vec::new();
    for back in backs_of(game_id) {
        if !labels.contains(&back.label) {
            continue;
        }
        loaded.push((back, load_card_back(back.asset_id).await?));
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

fn backs_of(game_id: &str) -> &'static [CardBack] {
    CARD_BACKS
        .iter()
        .find(|(id, _)| *id == game_id)
        .map_or(&[], |(_, backs)| *backs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn printing_from(collection: &str) -> Printing {
        Printing {
            card_id: "a-card".into(),
            card_title: "A Card".into(),
            is_official: true,
            variant: None,
            front: Default::default(),
            backs: Vec::new(),
            collection: collection.into(),
            back_group: "a-group".into(),
            pack_id: None,
            date_release: None,
            position: None,
        }
    }

    /// A game shipping at least two labels, so tests about withholding one of
    /// several cannot quietly turn into no-ops.
    fn a_game_with_several_labels() -> (&'static str, Vec<&'static str>) {
        CARD_BACKS
            .iter()
            .find_map(|(game_id, backs)| {
                let mut labels: Vec<&'static str> = backs.iter().map(|b| b.label).collect();
                labels.sort_unstable();
                labels.dedup();
                (labels.len() >= 2).then_some((*game_id, labels))
            })
            .expect("no game ships two card back labels")
    }

    fn a_game_that_ships_backs() -> (&'static str, &'static [CardBack]) {
        *CARD_BACKS
            .iter()
            .find(|(_, backs)| !backs.is_empty())
            .expect("no game ships card backs")
    }

    /// A database holding `collections`, each as (name, withheld labels).
    async fn db_with(game_id: &str, collections: &[(&str, &[&str])]) -> (TempDir, DbStorage) {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        db.initialize_schema().await.unwrap();

        for (id, (name, labels)) in collections.iter().enumerate() {
            let restricted = crate::db_storage::format_restricted_labels(
                &labels.iter().map(|l| l.to_string()).collect::<Vec<_>>(),
            )
            .map_or("NULL".to_string(), |l| format!("'{}'", l));

            db.execute(&format!(
                "INSERT INTO collections (id, name, game_id, version, language, restricted_back_labels, added_date)
                 VALUES ({}, '{}', '{}', '1.0.0', 'en', {}, '2026-01-01')",
                id + 1,
                name,
                game_id,
                restricted
            ))
            .await
            .unwrap();
        }

        (temp_dir, db)
    }

    #[tokio::test]
    async fn a_request_touching_no_restricted_collection_gets_every_label() {
        let (game_id, backs) = a_game_that_ships_backs();
        let (_dir, mut db) = db_with(game_id, &[("ffg", &[]), ("nsg", &[backs[0].label])]).await;

        let labels = allowed_labels(&mut db, game_id, &[printing_from("ffg")])
            .await
            .unwrap();

        assert!(labels.contains(&backs[0].label));
    }

    #[tokio::test]
    async fn one_collection_withholding_a_label_withholds_it_for_the_whole_request() {
        let (game_id, backs) = a_game_that_ships_backs();
        let withheld = backs[0].label;
        let (_dir, mut db) = db_with(game_id, &[("ffg", &[]), ("nsg", &[withheld])]).await;

        let printings = [printing_from("ffg"), printing_from("nsg")];
        let labels = allowed_labels(&mut db, game_id, &printings).await.unwrap();

        assert!(!labels.contains(&withheld));
    }

    #[tokio::test]
    async fn collections_withholding_different_labels_withhold_both() {
        let (game_id, every) = a_game_with_several_labels();
        let (_dir, mut db) = db_with(game_id, &[("one", &[every[0]]), ("two", &[every[1]])]).await;

        let printings = [printing_from("one"), printing_from("two")];
        let labels = allowed_labels(&mut db, game_id, &printings).await.unwrap();

        assert!(!labels.contains(&every[0]));
        assert!(!labels.contains(&every[1]));
    }

    #[tokio::test]
    async fn withholding_every_label_leaves_none() {
        let (game_id, backs) = a_game_that_ships_backs();
        let every: Vec<&str> = backs.iter().map(|b| b.label).collect();
        let (_dir, mut db) = db_with(game_id, &[("nsg", &every)]).await;

        let labels = allowed_labels(&mut db, game_id, &[printing_from("nsg")])
            .await
            .unwrap();

        assert!(labels.is_empty());
    }

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

    #[tokio::test]
    async fn a_label_carried_by_several_back_groups_is_listed_once() {
        let (game_id, every) = a_game_with_several_labels();
        let (_dir, mut db) = db_with(game_id, &[]).await;

        let labels = allowed_labels(&mut db, game_id, &[]).await.unwrap();

        assert_eq!(labels, every);
    }

    #[test]
    fn no_label_takes_proxy_then_the_first_offered() {
        assert_eq!(default_label(&["alt", "proxy"]), Some("proxy"));
        assert_eq!(default_label(&["alt", "zed"]), Some("alt"));
        assert_eq!(default_label(&[]), None);
    }

    #[test]
    fn every_shipped_back_is_found_by_its_own_group_and_label() {
        for (game_id, backs) in CARD_BACKS {
            for back in *backs {
                assert_eq!(
                    card_back(game_id, back.back_group, back.label),
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
        assert!(backs_of("no-such-game").is_empty());
        assert_eq!(card_back("no-such-game", "corp", "proxy"), None);

        for (game_id, backs) in CARD_BACKS {
            assert_eq!(
                card_back(game_id, "no-such-group", "proxy"),
                None,
                "{}",
                game_id
            );
            if let Some(back) = backs.first() {
                assert_eq!(
                    card_back(game_id, back.back_group, "no-such-label"),
                    None,
                    "{}",
                    game_id
                );
            }
        }
    }
}
