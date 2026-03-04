use crate::card_source::{CardSource, Cardlist, SetName};
use crate::catalog::normalize_title;
use crate::db_schema::build_placeholders;
use crate::models::{CardRequest, Printing};
use dirs;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use turso::{Connection, params, params_from_iter};

impl CardSource for Cardlist {
    async fn to_card_requests(
        &self,
        store: &CardStore,
    ) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let (requests, not_found) = store.parse_cardlist_into_card_requests(&self.0).await?;

        if !not_found.is_empty() {
            eprintln!("Warning: {} card(s) not found in catalog:", not_found.len());
            for name in &not_found {
                eprintln!("  - {}", name);
            }
            eprintln!("Consider running 'proxynexus catalog update'");
        }

        Ok(requests)
    }
}

impl CardSource for SetName {
    async fn to_card_requests(
        &self,
        store: &CardStore,
    ) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        store.get_card_requests_from_set_name(&self.0).await
    }
}

pub struct CardStore {
    collections_dir: PathBuf,
    conn: Connection,
}

type CardOverride<'a> = (&'a str, Option<String>, Option<String>, Option<String>);

impl CardStore {
    pub fn new(conn: Connection) -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Could not find home directory")?;
        let proxynexus_dir = home.join(".proxynexus");
        let collections_dir = proxynexus_dir.join("collections");

        Ok(Self {
            collections_dir,
            conn,
        })
    }

    async fn parse_cardlist_into_card_requests(
        &self,
        text: &str,
    ) -> Result<(Vec<CardRequest>, Vec<String>), Box<dyn std::error::Error>> {
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
                        code: code.clone(),
                        variant: variant.clone(),
                        collection: collection.clone(),
                        pack_code: requested_pack_code
                            .clone()
                            .or_else(|| Some(resolved_pack_code.clone())),
                    },
                    qty as usize,
                ));
            }
        }

        Ok((requests, not_found))
    }

    fn parse_quantity(line: &str) -> (u32, &str) {
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

    fn parse_overrides(text: &str) -> Result<CardOverride<'_>, Box<dyn std::error::Error>> {
        if let Some(bracket_start) = text.find('[') {
            let name = text[..bracket_start].trim();
            let bracket_end = text.find(']').ok_or("Unclosed bracket in card override")?;

            let inner = &text[bracket_start + 1..bracket_end];
            if inner.trim().is_empty() {
                return Err("Empty override brackets".into());
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
        &self,
        names: &[&str],
    ) -> Result<(HashMap<String, (String, String, String)>, Vec<String>), Box<dyn std::error::Error>>
    {
        let normalized_name_map: HashMap<&str, String> = names
            .iter()
            .map(|&name| (name, normalize_title(name)))
            .collect();

        let placeholders = build_placeholders(normalized_name_map.len());

        let query = format!(
            "SELECT c.code, c.title, c.pack_code, c.title_normalized
             FROM cards c
             JOIN packs p ON c.pack_code = p.code
             WHERE c.title_normalized IN ({})
             ORDER BY (p.date_release IS NULL) ASC, p.date_release DESC",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query).await?;
        let unique_normalized_name: HashSet<&str> =
            normalized_name_map.values().map(|s| s.as_str()).collect();
        let mut rows = stmt.query(params_from_iter(unique_normalized_name)).await?;

        let mut resolved_map: HashMap<String, (String, String, String)> = HashMap::new();
        while let Some(row) = rows.next().await? {
            let code: String = row.get(0)?;
            let title: String = row.get(1)?;
            let pack_code: String = row.get(2)?;
            let norm: String = row.get(3)?;

            resolved_map.entry(norm).or_insert((code, title, pack_code));
        }

        if resolved_map.is_empty() && !names.is_empty() {
            return Err(
                "No card titles found in the local catalog. Is your catalog seeded?".into(),
            );
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

    pub async fn get_available_packs(
        &self,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                pack_name,
                GROUP_CONCAT(coll_count || ' in ' || coll_name, ', ') as meta
             FROM (
                SELECT
                    p.name as pack_name,
                    p.code as pack_code,
                    col.name as coll_name,
                    COUNT(pr.card_code) as coll_count,
                    p.date_release
                FROM packs p
                JOIN cards c ON c.pack_code = p.code
                LEFT JOIN printings pr ON pr.card_code = c.code
                LEFT JOIN collections col ON pr.collection_id = col.id
                GROUP BY p.code, col.id
             )
             GROUP BY pack_code
             ORDER BY date_release",
            )
            .await?;
        let mut rows = stmt.query(()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            let name: String = row.get(0)?;
            let meta: Option<String> = row.get(1)?;

            let display_meta = meta
                .map(|m| format!("# {}", m))
                .unwrap_or_else(|| "# no printings available".to_string());

            results.push((name, display_meta));
        }

        Ok(results)
    }

    async fn get_card_requests_from_set_name(
        &self,
        set_name: &str,
    ) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.code, c.title, c.quantity
             FROM cards c
             JOIN packs p ON c.pack_code = p.code
             WHERE LOWER(p.name) = ?1
             ORDER BY c.code",
            )
            .await?;

        let mut rows = stmt.query(params![set_name.to_lowercase()]).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            let code: String = row.get(0)?;
            let title: String = row.get(1)?;
            let qty: u32 = row.get(2)?;

            results.extend(std::iter::repeat_n(
                CardRequest {
                    title: title.clone(),
                    code: code.clone(),
                    variant: None,
                    collection: None,
                    pack_code: None,
                },
                qty as usize,
            ));
        }

        if results.is_empty() {
            return Err(format!("No cards found for set '{}'", set_name).into());
        }

        Ok(results)
    }

    pub async fn resolve_codes_to_card_requests(
        &self,
        codes: &HashMap<String, u32>,
    ) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        if codes.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = build_placeholders(codes.len());

        let query = format!(
            "SELECT code, title FROM cards WHERE code IN ({})",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query).await?;
        let mut rows = stmt.query(params_from_iter(codes.keys().cloned())).await?;

        let mut resolved_titles = HashMap::new();
        while let Some(row) = rows.next().await? {
            let code: String = row.get(0)?;
            let title: String = row.get(1)?;
            resolved_titles.insert(code, title);
        }

        if resolved_titles.is_empty() && !codes.is_empty() {
            return Err("No card codes found in the local catalog. Is your catalog seeded?".into());
        }

        let mut requests = Vec::new();
        for (code, qty) in codes {
            if let Some(title) = resolved_titles.get(code) {
                requests.extend(std::iter::repeat_n(
                    CardRequest {
                        title: title.clone(),
                        code: code.clone(),
                        variant: None,
                        collection: None,
                        pack_code: None,
                    },
                    *qty as usize,
                ));
            } else {
                eprintln!(
                    "Warning: Card code '{}' from NetrunnerDB not found in local catalog",
                    code
                );
                eprintln!("  Consider running 'proxynexus catalog update'");
            }
        }

        Ok(requests)
    }

    pub async fn get_available_printings(
        &self,
        card_requests: &[CardRequest],
    ) -> Result<HashMap<String, Vec<Printing>>, Box<dyn std::error::Error>> {
        let unique_titles: HashSet<String> = card_requests
            .iter()
            .map(|r| normalize_title(&r.title))
            .collect();

        let placeholders = build_placeholders(unique_titles.len());
        let query = format!(
            "SELECT c.title, c.code, p.variant, p.file_path, col.name, c.side, c.pack_code
             FROM printings p
             JOIN cards c ON p.card_code = c.code
             JOIN collections col ON p.collection_id = col.id
             JOIN packs pks ON c.pack_code = pks.code
             WHERE c.title_normalized IN ({})
             ORDER BY
                CASE WHEN p.variant = 'original' THEN 0 ELSE 1 END,
                pks.date_release DESC,
                col.added_date",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query).await?;
        let mut rows = stmt.query(params_from_iter(unique_titles)).await?;

        let mut resolved_printings: HashMap<String, Vec<Printing>> = HashMap::new();
        while let Some(row) = rows.next().await? {
            let title: String = row.get(0)?;
            let normalized = normalize_title(&title);
            let relative_path: String = row.get(3)?;
            let printing = Printing {
                card_title: title,
                card_code: row.get(1)?,
                variant: row.get(2)?,
                file_path: self.collections_dir.join(relative_path),
                collection: row.get(4)?,
                side: row.get(5)?,
                pack_code: row.get(6)?,
            };
            resolved_printings
                .entry(normalized)
                .or_default()
                .push(printing);
        }

        if resolved_printings.is_empty() && !card_requests.is_empty() {
            return Err("No printings found in your collections for any requested cards.".into());
        }

        let mut missing_titles = HashSet::new();
        for req in card_requests {
            let norm = normalize_title(&req.title);
            if !resolved_printings.contains_key(&norm) && missing_titles.insert(norm) {
                eprintln!(
                    "Warning: No printings found for '{}' in your collections.",
                    req.title
                );
            }
        }

        Ok(resolved_printings)
    }

    pub fn resolve_printings(
        &self,
        requests: &[CardRequest],
        available: &HashMap<String, Vec<Printing>>,
    ) -> Result<Vec<Printing>, Box<dyn std::error::Error>> {
        let mut result = Vec::new();

        for request in requests {
            let normalized = normalize_title(&request.title);

            if let Some(printings) = available.get(&normalized) {
                match Self::select_printing(request, printings) {
                    Ok(printing) => result.push(printing),
                    Err(e) => {
                        eprintln!("Warning: {}", e);
                        if let Some(fallback) = printings.first() {
                            eprintln!("  Using: {} from {}", fallback.variant, fallback.collection);
                            result.push(fallback.clone());
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    pub fn select_printing(
        request: &CardRequest,
        printings: &[Printing],
    ) -> Result<Printing, Box<dyn std::error::Error>> {
        let mut candidates: Vec<&Printing> = printings.iter().collect();

        if let Some(ref target_set) = request.pack_code {
            let mut set_matches = candidates.clone();
            set_matches.retain(|p| &p.pack_code == target_set);

            if !set_matches.is_empty() {
                candidates = set_matches;
            }
        }

        if let Some(ref target_variant) = request.variant {
            let mut variant_matches = candidates.clone();
            variant_matches.retain(|p| &p.variant == target_variant);

            if !variant_matches.is_empty() {
                candidates = variant_matches;
            }
        }

        if let Some(ref target_coll) = request.collection {
            let mut coll_matches = candidates.clone();
            coll_matches.retain(|p| &p.collection == target_coll);

            if !coll_matches.is_empty() {
                candidates = coll_matches;
            }
        }

        candidates
            .first()
            .map(|p| (*p).clone())
            .ok_or_else(|| format!("No matching printing found for '{}'", request.title).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Printing;

    fn mock_printing(variant: &str, coll: &str, pack: &str) -> Printing {
        Printing {
            card_title: "Sure Gamble".into(),
            card_code: "01050".into(),
            variant: variant.into(),
            file_path: PathBuf::from("01050.jpg"),
            collection: coll.into(),
            side: "runner".into(),
            pack_code: pack.into(),
        }
    }

    #[test]
    fn test_select_printing_prioritization() {
        let p1 = mock_printing("original", "ffg-en", "core");
        let p2 = mock_printing("alt1", "standard", "core");
        let p3 = mock_printing("original", "alt-arts", "core");
        let available = vec![p1.clone(), p2.clone(), p3.clone()];

        // Exact variant match
        let req = CardRequest {
            title: "Sure Gamble".into(),
            code: "01050".into(),
            variant: Some("alt1".into()),
            collection: None,
            pack_code: None,
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
            code: "01050".into(),
            variant: None,
            collection: Some("alt-arts".into()),
            pack_code: None,
        };
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .collection,
            "alt-arts"
        );

        // When requested variant doesn't exist
        let req = CardRequest {
            title: "Sure Gamble".into(),
            code: "01050".into(),
            variant: Some("nonexistent".into()),
            collection: None,
            pack_code: None,
        };
        // Return the first item found
        assert_eq!(
            CardStore::select_printing(&req, &available)
                .unwrap()
                .variant,
            "original"
        );
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
        assert_eq!(v, Some("alt".to_string()));
        assert_eq!(c, Some("ffg-en".to_string()));
        assert_eq!(p, Some("core".to_string()));

        // Partial, variant only
        let (_, v, c, p) = CardStore::parse_overrides("Sure Gamble [alt]").unwrap();
        assert_eq!(v, Some("alt".to_string()));
        assert_eq!(c, None);
        assert_eq!(p, None);

        // Partial, skipped slots
        let (_, v, c, p) = CardStore::parse_overrides("Sure Gamble [:std:]").unwrap();
        assert_eq!(v, None);
        assert_eq!(c, Some("std".to_string()));
        assert_eq!(p, None);

        // Case normalization in overrides
        let (_, v, _, _) = CardStore::parse_overrides("Card [ALT]").unwrap();
        assert_eq!(v, Some("alt".to_string()));
    }
}
