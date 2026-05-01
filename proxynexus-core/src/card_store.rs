use crate::card_source::{CardSource, Cardlist, SetName};
use crate::db_storage::{DbStorage, build_in_clause, quote_sql_string};
use crate::error::{ProxyNexusError, Result};
use crate::models::{CardRequest, Decklist, Printing, PrintingPart};
use gluesql::FromGlueRow;
use gluesql::core::row_conversion::SelectExt;
use std::collections::{HashMap, HashSet};
use std::string::String;
use tracing::warn;

#[derive(FromGlueRow)]
struct PackRow {
    pack_name: String,
    pack_id: String,
    coll_name: Option<String>,
    coll_count: i64,
    date_release: Option<String>,
}

#[derive(FromGlueRow)]
struct CardNameRow {
    id: String,
    title: String,
    pack_id: String,
    title_normalized: String,
}

#[derive(FromGlueRow)]
struct CardRequestRow {
    id: String,
    title: String,
    quantity: i64,
    pack_id: String,
}

#[derive(FromGlueRow)]
struct CardRow {
    id: String,
    title: String,
}

#[derive(FromGlueRow)]
struct CardTitleRow {
    title: String,
}

#[derive(FromGlueRow)]
struct AvailablePrintingRow {
    title: String,
    id: String,
    is_official: bool,
    variant: Option<String>,
    file_path: String,
    part: String,
    name: String,
    side: String,
    pack_id: Option<String>,
    date_release: Option<String>,
}

pub fn normalize_title(title: &str) -> String {
    deunicode::deunicode(title)
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

pub fn clean_card_name(name: &str) -> &str {
    name.trim_end_matches(|c: char| !c.is_alphanumeric() && !"!.*)\"'”’“‘".contains(c))
}

impl CardSource for Cardlist {
    async fn to_card_requests(&self, store: &mut CardStore<'_>) -> Result<Vec<CardRequest>> {
        let (requests, not_found) = store.parse_cardlist_into_card_requests(&self.0).await?;

        if !not_found.is_empty() {
            warn!(
                "{} card(s) not found in catalog: {:?}",
                not_found.len(),
                not_found
            );
            warn!("Consider running 'proxynexus catalog update'");
        }

        Ok(requests)
    }
}

impl CardSource for SetName {
    async fn to_card_requests(&self, store: &mut CardStore<'_>) -> Result<Vec<CardRequest>> {
        store.get_card_requests_from_set_name(&self.0).await
    }
}

pub struct CardStore<'a> {
    db: &'a mut DbStorage,
    pub active_game_id: String,
}

type CardOverride<'a> = (&'a str, Option<String>, Option<String>, Option<String>);

impl<'a> CardStore<'a> {
    pub fn new(db: &'a mut DbStorage, active_game_id: String) -> Result<Self> {
        Ok(Self { db, active_game_id })
    }

    pub async fn get_all_card_names(&mut self) -> Result<Vec<String>> {
        let query = format!(
            "SELECT DISTINCT title
            FROM cards
            WHERE game_id = {}
            ORDER BY title",
            quote_sql_string(&self.active_game_id)
        );
        let payloads = self.db.execute(&query).await?;
        let mut names = Vec::new();

        if let Some(payload) = payloads.into_iter().next() {
            names = payload
                .rows_as::<CardTitleRow>()?
                .into_iter()
                .map(|row| row.title)
                .collect();
        }

        Ok(names)
    }

    async fn parse_cardlist_into_card_requests(
        &mut self,
        text: &str,
    ) -> Result<(Vec<CardRequest>, Vec<String>)> {
        type CardlistEntry<'a> = (&'a str, u32, Option<String>, Option<String>, Option<String>);
        let mut entries: Vec<CardlistEntry> = Vec::new();

        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            let (qty, rest) = Self::parse_quantity(line);
            let (name, variant_pref, collection_pref, pack_code_pref) =
                Self::parse_overrides(rest)?;

            let name = clean_card_name(name);
            entries.push((name, qty, variant_pref, collection_pref, pack_code_pref));
        }

        if entries.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let titles: Vec<&str> = entries.iter().map(|(name, ..)| *name).collect();
        let (resolved_cards, not_found) = self.resolve_names_to_cards(&titles).await?;

        let mut requests = Vec::new();

        for (name, qty, variant, collection, requested_pack_code) in entries {
            if let Some((code, title, resolved_pack_code)) = resolved_cards.get(name) {
                requests.extend(std::iter::repeat_n(
                    CardRequest {
                        title: title.clone(),
                        id: code.clone(),
                        variant: variant.clone(),
                        collection: collection.clone(),
                        pack_id: requested_pack_code
                            .clone()
                            .or_else(|| Some(resolved_pack_code.clone())),
                    },
                    qty as usize,
                ));
            }
        }

        Ok((requests, not_found))
    }

    pub fn parse_quantity(line: &str) -> (u32, &str) {
        if let Some((qty_str, card_name)) = line
            .split_once("x ")
            .filter(|(qty_str, _)| qty_str.chars().all(|c| c.is_ascii_digit()))
        {
            let qty: u32 = qty_str.parse().unwrap_or(1);
            (qty, card_name.trim())
        } else if let Some((prefix, rest)) = line.split_once(' ') {
            if prefix.chars().all(|c| c.is_ascii_digit()) {
                let qty: u32 = prefix.parse().unwrap_or(1);
                (qty, rest.trim())
            } else {
                (1, line)
            }
        } else {
            (1, line)
        }
    }

    pub fn parse_overrides(text: &str) -> Result<CardOverride<'_>> {
        if let Some(bracket_start) = text.find('[') {
            let name = text[..bracket_start].trim();
            let bracket_end = text.find(']').ok_or_else(|| {
                ProxyNexusError::Internal("Unclosed bracket in card override".into())
            })?;

            let inner = &text[bracket_start + 1..bracket_end];
            if inner.trim().is_empty() {
                return Err(ProxyNexusError::Internal("Empty override brackets".into()));
            }

            let parts: Vec<Option<String>> = inner
                .split(':')
                .map(|s| {
                    let cleaned = s.trim().to_lowercase();
                    if cleaned.is_empty() {
                        None
                    } else {
                        Some(cleaned)
                    }
                })
                .collect();

            let variant = parts.first().cloned().flatten();
            let collection = parts.get(1).cloned().flatten();
            let pack_code = parts.get(2).cloned().flatten();

            Ok((name, variant, collection, pack_code))
        } else {
            Ok((text.trim(), None, None, None))
        }
    }

    async fn resolve_names_to_cards(
        &mut self,
        names: &[&str],
    ) -> Result<(HashMap<String, (String, String, String)>, Vec<String>)> {
        let normalized_name_map: HashMap<&str, String> = names
            .iter()
            .map(|&name| (name, normalize_title(name)))
            .collect();

        let unique_normalized_name: HashSet<&str> =
            normalized_name_map.values().map(|s| s.as_str()).collect();
        let in_clause = build_in_clause(unique_normalized_name);

        let query = format!(
            "SELECT 
                c.id, 
                c.title, 
                v.pack_id, 
                c.title_normalized
             FROM cards c
             JOIN card_versions v ON c.id = v.card_id
             JOIN packs p ON v.pack_id = p.id
             WHERE c.title_normalized IN ({})
               AND c.game_id = {}
             ORDER BY
                 CASE WHEN p.date_release IS NULL THEN 1 ELSE 0 END,
                 p.date_release DESC",
            in_clause,
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut resolved_map: HashMap<String, (String, String, String)> = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            let name_rows = payload.rows_as::<CardNameRow>()?;
            for row in name_rows {
                resolved_map.entry(row.title_normalized).or_insert((
                    row.id,
                    row.title,
                    row.pack_id,
                ));
            }
        }

        if resolved_map.is_empty() && !names.is_empty() {
            return Err(ProxyNexusError::Internal(
                "No card titles found in the local catalog. Is your catalog seeded?".into(),
            ));
        }

        let mut title_to_card: HashMap<String, (String, String, String)> = HashMap::new();
        let mut not_found = Vec::new();

        for (&title, normalized) in &normalized_name_map {
            if let Some(card_data) = resolved_map.get(normalized) {
                title_to_card.insert(title.to_string(), card_data.clone());
            } else {
                not_found.push(title.to_string());
            }
        }

        Ok((title_to_card, not_found))
    }

    pub async fn get_available_packs(&mut self) -> Result<Vec<(String, String, String)>> {
        let query = format!(
            "SELECT
                p.name as pack_name,
                p.id as pack_id,
                col.name AS coll_name,
                COUNT(pr.id) as coll_count,
                p.date_release
            FROM packs p
            JOIN card_versions v ON p.id = v.pack_id
            LEFT JOIN printings pr ON pr.version_id = v.id
            LEFT JOIN collections col ON pr.collection_id = col.id
            WHERE p.game_id = {}
            GROUP BY p.id, col.id",
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;

        struct PackGroup {
            id: String,
            name: String,
            date_release: String,
            collections: Vec<String>,
        }

        let mut pack_data: HashMap<String, PackGroup> = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            let pack_rows = payload.rows_as::<PackRow>()?;

            for row in pack_rows {
                let date_release = row.date_release.unwrap_or_default();

                let entry = pack_data
                    .entry(row.pack_id.clone())
                    .or_insert_with(|| PackGroup {
                        id: row.pack_id.clone(),
                        name: row.pack_name,
                        date_release,
                        collections: Vec::new(),
                    });

                if let Some(name) = row.coll_name
                    && row.coll_count > 0
                {
                    entry
                        .collections
                        .push(format!("{} in {}", row.coll_count, name));
                }
            }
        }

        let mut sorted_packs: Vec<_> = pack_data.into_values().collect();
        sorted_packs.sort_by(|a, b| a.date_release.cmp(&b.date_release));

        let mut results = Vec::new();

        for mut pack in sorted_packs {
            pack.collections.sort();
            let meta = if pack.collections.is_empty() {
                None
            } else {
                Some(pack.collections.join(", "))
            };

            let display_meta = meta
                .map(|m| format!("# {}", m))
                .unwrap_or_else(|| "# no printings available".to_string());

            results.push((pack.name, pack.id, display_meta));
        }

        Ok(results)
    }

    async fn get_card_requests_from_set_name(
        &mut self,
        set_name: &str,
    ) -> Result<Vec<CardRequest>> {
        let query = format!(
            "SELECT c.id, c.title, v.quantity, v.pack_id
             FROM cards c
             JOIN card_versions v ON c.id = v.card_id
             JOIN packs p ON v.pack_id = p.id
             WHERE LOWER(p.name) = {}
               AND c.game_id = {}
             ORDER BY c.id",
            quote_sql_string(&set_name.to_lowercase()),
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut results = Vec::new();

        if let Some(payload) = payloads.into_iter().next() {
            let request_rows = payload.rows_as::<CardRequestRow>()?;

            for row in request_rows {
                results.extend(std::iter::repeat_n(
                    CardRequest {
                        title: row.title,
                        id: row.id,
                        variant: None,
                        collection: None,
                        pack_id: Some(row.pack_id),
                    },
                    row.quantity as usize,
                ));
            }
        }

        if results.is_empty() {
            return Err(ProxyNexusError::Internal(format!(
                "No cards found for set '{}'",
                set_name
            )));
        }

        Ok(results)
    }

    pub async fn resolve_decklist_to_requests(
        &mut self,
        decklist: &Decklist,
    ) -> Result<Vec<CardRequest>> {
        if decklist.cards.is_empty() {
            return Ok(Vec::new());
        }

        let in_clause = build_in_clause(decklist.cards.keys());

        let query = format!(
            "SELECT c.id, c.title
             FROM cards c
             WHERE c.id IN ({})
               AND c.game_id = {}",
            in_clause,
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut resolved_titles = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            let card_rows = payload.rows_as::<CardRow>()?;
            for row in card_rows {
                resolved_titles.insert(row.id, row.title);
            }
        }

        if resolved_titles.is_empty() && !decklist.cards.is_empty() {
            return Err(ProxyNexusError::Internal(
                "No card IDs found in the local catalog.".into(),
            ));
        }

        let mut requests = Vec::new();
        for (id, qty) in &decklist.cards {
            if let Some(title) = resolved_titles.get(id) {
                requests.extend(std::iter::repeat_n(
                    CardRequest {
                        title: title.clone(),
                        id: id.clone(),
                        variant: None,
                        collection: None,
                        pack_id: None,
                    },
                    *qty as usize,
                ));
            } else {
                warn!("Card ID '{}' from decklist not found in local catalog", id);
            }
        }

        Ok(requests)
    }

    pub async fn get_available_printings(
        &mut self,
        card_requests: &[CardRequest],
    ) -> Result<HashMap<String, Vec<Printing>>> {
        let unique_titles: HashSet<String> = card_requests
            .iter()
            .map(|r| normalize_title(&r.title))
            .collect();

        let in_clause = build_in_clause(&unique_titles);

        let query = format!(
            "SELECT 
                c.title, 
                c.id,
                p.is_official,
                p.variant, 
                p.file_path, 
                p.part, 
                col.name,
                c.side, 
                v.pack_id,
                pks.date_release
             FROM printings p
             JOIN cards c ON p.card_id = c.id
             JOIN collections col ON p.collection_id = col.id
             LEFT JOIN card_versions v ON p.version_id = v.id
             LEFT JOIN packs pks ON v.pack_id = pks.id
             WHERE c.title_normalized IN ({})
               AND c.game_id = {}",
            in_clause,
            quote_sql_string(&self.active_game_id)
        );

        let payloads = self.db.execute(&query).await?;
        let mut resolved_printings: HashMap<String, Vec<Printing>> = HashMap::new();

        if let Some(payload) = payloads.into_iter().next() {
            let printing_rows = payload.rows_as::<AvailablePrintingRow>()?;
            resolved_printings = Self::assemble_printings(printing_rows);
        }

        if resolved_printings.is_empty() && !card_requests.is_empty() {
            return Err(ProxyNexusError::Internal(
                "No printings found in your collections for any requested cards.".into(),
            ));
        }

        let mut missing_titles = HashSet::new();
        for req in card_requests {
            let norm = normalize_title(&req.title);
            if !resolved_printings.contains_key(&norm) && missing_titles.insert(norm) {
                warn!(
                    "No printings found for '{}' in your collections.",
                    req.title
                );
            }
        }

        Ok(resolved_printings)
    }

    fn assemble_printings(rows: Vec<AvailablePrintingRow>) -> HashMap<String, Vec<Printing>> {
        let mut resolved_printings: HashMap<String, Vec<Printing>> = HashMap::new();
        let mut groups: HashMap<
            (String, String, Option<String>, String),
            Vec<AvailablePrintingRow>,
        > = HashMap::new();

        for row in rows {
            let normalized = normalize_title(&row.title);
            let key = (
                normalized,
                row.id.clone(),
                row.variant.clone(),
                row.name.clone(),
            );
            groups.entry(key).or_default().push(row);
        }

        for ((normalized, card_id, variant, collection), rows) in groups {
            let mut image_key = String::new();
            let mut parts = Vec::new();

            let first_row = &rows[0];
            let card_title = first_row.title.clone();
            let is_official = first_row.is_official;
            let side = first_row.side.clone();
            let pack_id = first_row.pack_id.clone();
            let date_release = first_row.date_release.clone();

            for row in rows {
                if row.part == "front" {
                    image_key = row.file_path;
                } else {
                    parts.push(PrintingPart {
                        name: row.part,
                        image_key: row.file_path,
                    });
                }
            }

            let printing = Printing {
                card_title,
                card_id,
                is_official,
                variant: variant.unwrap_or_else(|| "original".to_string()),
                image_key,
                parts,
                collection,
                side,
                pack_id,
                date_release,
            };

            resolved_printings
                .entry(normalized)
                .or_default()
                .push(printing);
        }

        for printings in resolved_printings.values_mut() {
            printings.sort_by_key(|p| (p.date_release.is_none(), p.date_release.clone()));
        }

        resolved_printings
    }

    pub fn resolve_printings(
        &self,
        requests: &[CardRequest],
        available: &HashMap<String, Vec<Printing>>,
    ) -> Result<Vec<Printing>> {
        let mut result = Vec::new();

        for request in requests {
            let normalized = normalize_title(&request.title);

            if let Some(printings) = available.get(&normalized) {
                match Self::select_printing(request, printings) {
                    Ok(printing) => result.push(printing),
                    Err(e) => {
                        warn!("{}", e);
                        if let Some(fallback) = printings.first() {
                            warn!("  Using: {} from {}", fallback.variant, fallback.collection);
                            result.push(fallback.clone());
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    pub fn select_printing(request: &CardRequest, printings: &[Printing]) -> Result<Printing> {
        let mut candidates: Vec<&Printing> = printings.iter().collect();

        let target_variant = request.variant.as_deref().unwrap_or("original");

        candidates.sort_by_key(|p| {
            (
                p.variant != target_variant,
                p.variant != "original",
                request.collection.is_some() && request.collection.as_ref() != Some(&p.collection),
                request.pack_id.is_some() && request.pack_id != p.pack_id,
                p.card_id != request.id,
                p.date_release.is_none(),
                p.date_release.clone(),
            )
        });

        candidates.into_iter().next().cloned().ok_or_else(|| {
            ProxyNexusError::Internal(format!(
                "No matching printing found for '{}'",
                request.title
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Printing;

    fn mock_printing(
        code: &str,
        variant: &str,
        coll: &str,
        pack: &str,
        date: Option<&str>,
    ) -> Printing {
        Printing {
            card_title: "Sure Gamble".into(),
            card_id: code.into(),
            is_official: true,
            variant: variant.into(),
            image_key: format!("{}.jpg", code),
            parts: Vec::new(),
            collection: coll.into(),
            side: "runner".into(),
            pack_id: Some(pack.into()),
            date_release: date.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_select_printing_prioritization() {
        let p1 = mock_printing("01050", "original", "ffg-en", "core", Some("2012-12-01"));
        let p2 = mock_printing("01050", "alt1", "standard", "core", Some("2012-12-01"));
        let p3 = mock_printing(
            "20050",
            "original",
            "alt-arts",
            "revised",
            Some("2017-01-01"),
        );
        let p_collection = mock_printing(
            "01050",
            "original",
            "alt-arts",
            "revised",
            Some("2017-01-01"),
        );

        let available = vec![p1.clone(), p2.clone(), p3.clone(), p_collection.clone()];

        // Exact variant match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "01050".into(),
            variant: Some("alt1".into()),
            collection: None,
            pack_id: None,
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .variant,
            "alt1"
        );

        // Exact collection match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "01050".into(),
            variant: None,
            collection: Some("alt-arts".into()),
            pack_id: None,
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .collection,
            "alt-arts"
        );

        // Exact pack match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "01050".into(),
            variant: None,
            collection: None,
            pack_id: Some("core".to_string()),
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .pack_id,
            Some("core".to_string())
        );

        // Variant Fallback: If 'core' original is missing, pick 'revised' original over 'core' alt
        let available_missing_core_orig = vec![p2.clone(), p3.clone()];
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "01050".into(),
            variant: Some("original".into()),
            collection: None,
            pack_id: Some("core".to_string()),
        };
        let result = CardStore::select_printing(&req, &available_missing_core_orig).unwrap();
        assert_eq!(result.variant, "original");

        // Default to the earliest original
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "01050".into(),
            variant: None,
            collection: None,
            pack_id: None,
        };
        let result = CardStore::select_printing(&req, &available).unwrap();
        assert_eq!(result.variant, "original");

        // Variant Match beats Exact ID Match
        let p4_revised_alt =
            mock_printing("20050", "alt2", "standard", "revised", Some("2017-01-01"));
        let available_mixed = vec![p1.clone(), p4_revised_alt.clone()];
        let req = CardRequest {
            title: "Sure Gamble".into(),
            id: "20050".into(),
            variant: Some("original".into()),
            collection: None,
            pack_id: None,
        };
        let result = CardStore::select_printing(&req, &available_mixed).unwrap();
        assert_eq!(result.card_id, "01050");
        assert_eq!(result.variant, "original");
    }

    #[test]
    fn test_select_printing_fallback_logic() {
        let p1 = mock_printing("1", "original", "c1", "p1", Some("2020-01-01"));
        let p2 = mock_printing("2", "alt1", "c1", "p2", Some("2021-01-01"));
        let p3 = mock_printing("3", "promo", "c2", "p3", Some("2022-01-01"));
        let available = vec![p1.clone(), p2.clone(), p3.clone()];

        // 1. Missing variant fallback to "original"
        let req1 = CardRequest {
            title: "Test".into(),
            id: "2".into(), // matches alt1 code
            variant: Some("missing_variant".into()),
            collection: None,
            pack_id: None,
        };
        let result1 = CardStore::select_printing(&req1, &available).unwrap();
        assert_eq!(result1.variant, "original");
        assert_eq!(result1.card_id, "1");

        // 2. Collection override beats default card code
        let p4 = mock_printing("4", "original", "c2", "p4", Some("2023-01-01"));
        let available2 = vec![p1.clone(), p4.clone()];
        let req3 = CardRequest {
            title: "Test".into(),
            id: "1".into(), // matches p1 (collection c1)
            variant: Some("original".into()),
            collection: Some("c2".into()), // requests collection c2
            pack_id: None,
        };
        let result3 = CardStore::select_printing(&req3, &available2).unwrap();
        assert_eq!(result3.card_id, "4");
        assert_eq!(result3.collection, "c2");
    }

    #[test]
    fn test_clean_card_name() {
        // valid trailing characters remain
        assert_eq!(clean_card_name("Snare!"), "Snare!");
        assert_eq!(clean_card_name("Eli 1.0"), "Eli 1.0");
        assert_eq!(
            clean_card_name("The World is Yours*"),
            "The World is Yours*"
        );
        assert_eq!(clean_card_name("Masterwork (v37)"), "Masterwork (v37)");
        assert_eq!(
            clean_card_name("\"Freedom Through Equality\""),
            "\"Freedom Through Equality\""
        );
        assert_eq!(
            clean_card_name("Title (with parens)"),
            "Title (with parens)"
        );

        // invalid trailing characters get stripped
        assert_eq!(clean_card_name("Hedge Fund ●"), "Hedge Fund");
        assert_eq!(clean_card_name("Sure Gamble -"), "Sure Gamble");
        assert_eq!(clean_card_name("Paperclip ●●●"), "Paperclip");
        assert_eq!(clean_card_name("Card Name ! ●"), "Card Name !");
        assert_eq!(clean_card_name("Card Name ●●●"), "Card Name");
    }

    #[test]
    fn test_parse_quantity() {
        assert_eq!(
            CardStore::parse_quantity("3x Sure Gamble"),
            (3, "Sure Gamble")
        );
        assert_eq!(
            CardStore::parse_quantity("3 Sure Gamble"),
            (3, "Sure Gamble")
        );
        assert_eq!(CardStore::parse_quantity("Sure Gamble"), (1, "Sure Gamble"));
        assert_eq!(
            CardStore::parse_quantity("10x Hedge Fund"),
            (10, "Hedge Fund")
        );
    }

    #[test]
    fn test_parse_overrides() {
        // Full override
        let (name, v, c, p) = CardStore::parse_overrides("Sure Gamble [alt:ffg-en:core]").unwrap();
        assert_eq!(name, "Sure Gamble");
        assert_eq!(v, Some("alt".into()));
        assert_eq!(c, Some("ffg-en".into()));
        assert_eq!(p, Some("core".into()));

        // Partial, variant only
        let (_, v, c, p) = CardStore::parse_overrides("Sure Gamble [alt]").unwrap();
        assert_eq!(v, Some("alt".into()));
        assert_eq!(c, None);
        assert_eq!(p, None);

        // Partial, skipped slots
        let (_, v, c, p) = CardStore::parse_overrides("Sure Gamble [:std:]").unwrap();
        assert_eq!(v, None);
        assert_eq!(c, Some("std".into()));
        assert_eq!(p, None);

        // Case normalization in overrides
        let (_, v, _, _) = CardStore::parse_overrides("Card [ALT]").unwrap();
        assert_eq!(v, Some("alt".into()));
    }

    #[test]
    fn test_normalize_title() {
        assert_eq!(normalize_title("Sure Gamble"), "sure_gamble");
        assert_eq!(normalize_title("Snare!"), "snare_");
        assert_eq!(normalize_title("Café"), "cafe");
        assert_eq!(normalize_title("piñata"), "pinata");
    }

    fn get_mock_available_printings() -> HashMap<String, Vec<Printing>> {
        let mut available = HashMap::new();
        let p1 = mock_printing("01050", "original", "ffg-en", "core", Some("2012-12-01"));
        let p2 = mock_printing("01050", "alt1", "standard", "core", Some("2012-12-01"));
        let p3 = mock_printing(
            "20050",
            "original",
            "alt-arts",
            "revised",
            Some("2017-01-01"),
        );
        let p_collection = mock_printing(
            "01050",
            "original",
            "alt-arts",
            "revised",
            Some("2017-01-01"),
        );
        available.insert("sure_gamble".to_string(), vec![p1, p2, p3, p_collection]);
        available
    }

    #[test]
    fn test_resolve_printings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = DbStorage::new_sled(temp_dir.path()).unwrap();
        let store = CardStore::new(&mut db, "netrunner".to_string()).unwrap();

        let mut available = get_mock_available_printings();
        available.insert(
            "snare_".to_string(),
            vec![mock_printing(
                "01051",
                "original",
                "ffg-en",
                "core",
                Some("2012-12-01"),
            )],
        );

        let req1 = CardRequest {
            title: "Sure Gamble".into(),
            id: "01050".into(),
            variant: None,
            collection: None,
            pack_id: None,
        };
        let req2 = CardRequest {
            title: "Missing Card".into(),
            id: "99999".into(),
            variant: None,
            collection: None,
            pack_id: None,
        };
        let req3 = CardRequest {
            title: "Snare!".into(),
            id: "01051".into(),
            variant: None,
            collection: None,
            pack_id: None,
        };

        let result = store
            .resolve_printings(&[req1, req2, req3], &available)
            .unwrap();

        // Only 2 printings resolved, missing card was skipped safely
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].card_id, "01050");
        assert_eq!(result[1].card_id, "01051");
    }
}
