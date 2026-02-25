use crate::card_source::{CardSource, Cardlist, SetName};
use crate::catalog::normalize_title;
use crate::models::{CardRequest, Printing};
use dirs;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct CardStore {
    app_db_path: PathBuf,
    collections_dir: PathBuf,
}

impl CardSource for Cardlist {
    fn to_card_requests(&self) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let store = CardStore::new()?;
        let (requests, not_found) = store.parse_cardlist_into_card_requests(&self.0)?;

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
    fn to_card_requests(&self) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let store = CardStore::new()?;
        store.get_card_requests_from_set_name(&self.0)
    }
}

impl CardStore {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Could not find home directory")?;
        let proxynexus_dir = home.join(".proxynexus");
        let collections_dir = proxynexus_dir.join("collections");
        let app_db_path = proxynexus_dir.join("proxynexus.db");

        Ok(Self {
            app_db_path,
            collections_dir,
        })
    }

    fn parse_cardlist_into_card_requests(
        &self,
        text: &str,
    ) -> Result<(Vec<CardRequest>, Vec<String>), Box<dyn std::error::Error>> {
        let mut entries: Vec<(&str, u32, Option<String>, Option<String>, Option<String>)> =
            Vec::new();

        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            let (qty, rest) = self.parse_quantity(line);
            let (name, variant_pref, collection_pref, pack_code_pref) =
                self.parse_overrides(rest)?;

            entries.push((name, qty, variant_pref, collection_pref, pack_code_pref));
        }

        let unique_titles: HashSet<&str> = entries.iter().map(|(name, ..)| *name).collect();
        let titles: Vec<&str> = unique_titles.into_iter().collect();

        let (resolved_cards, not_found) = self.resolve_names_to_cards(&titles)?;

        let mut requests = Vec::new();

        for (name, qty, variant, collection, requested_pack_code) in entries {
            if let Some((code, title, resolved_pack_code)) = resolved_cards.get(name) {
                for _ in 0..qty {
                    requests.push(CardRequest {
                        title: title.clone(),
                        code: code.clone(),
                        variant: variant.clone(),
                        collection: collection.clone(),
                        pack_code: requested_pack_code
                            .clone()
                            .or_else(|| Some(resolved_pack_code.clone())),
                    });
                }
            }
        }

        Ok((requests, not_found))
    }

    fn parse_quantity<'a>(&self, line: &'a str) -> (u32, &'a str) {
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

    fn parse_overrides<'a>(
        &self,
        text: &'a str,
    ) -> Result<(&'a str, Option<String>, Option<String>, Option<String>), Box<dyn std::error::Error>>
    {
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

            let variant = parts.get(0).cloned().flatten();
            let collection = parts.get(1).cloned().flatten();
            let pack_code = parts.get(2).cloned().flatten();

            Ok((name, variant, collection, pack_code))
        } else {
            Ok((text.trim(), None, None, None))
        }
    }

    fn resolve_names_to_cards(
        &self,
        names: &[&str],
    ) -> Result<(HashMap<String, (String, String, String)>, Vec<String>), Box<dyn std::error::Error>>
    {
        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let mut title_to_card: HashMap<String, (String, String, String)> = HashMap::new();
        let mut not_found = Vec::new();

        for title in names {
            let normalized = normalize_title(title);

            let result: Option<(String, String, String)> = conn
                .query_row(
                    "SELECT c.code, c.title, c.pack_code
                     FROM cards c
                     JOIN packs p ON c.pack_code = p.code
                     WHERE c.title_normalized = ?1
                     ORDER BY p.date_release DESC NULLS LAST
                     LIMIT 1",
                    params![normalized],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;

            match result {
                Some(card_data) => {
                    title_to_card.insert(title.to_string(), card_data);
                }
                None => not_found.push(title.to_string()),
            }
        }

        Ok((title_to_card, not_found))
    }

    pub fn get_available_packs(&self) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let mut stmt = conn.prepare(
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
        )?;

        let results = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let meta: Option<String> = row.get(1)?;

                let display_meta = match meta {
                    Some(m) => format!("# {}", m),
                    None => "# no printings available".to_string(),
                };

                Ok((name, display_meta))
            })?
            .collect::<Result<Vec<(String, String)>, _>>()?;

        Ok(results)
    }

    fn get_card_requests_from_set_name(
        &self,
        set_name: &str,
    ) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let mut stmt = conn.prepare(
            "SELECT c.code, c.title, c.quantity
             FROM cards c
             JOIN packs p ON c.pack_code = p.code
             WHERE LOWER(p.name) = ?1
             ORDER BY c.code",
        )?;

        let rows = stmt
            .query_map(params![set_name.to_lowercase()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<(String, String, u32)>, _>>()?;

        if rows.is_empty() {
            return Err(format!("No cards found for set '{}'", set_name).into());
        }

        Ok(rows
            .into_iter()
            .flat_map(|(code, title, qty)| {
                std::iter::repeat(CardRequest {
                    title: title.clone(),
                    code: code.clone(),
                    variant: None,
                    collection: None,
                    pack_code: None,
                })
                .take(qty as usize)
            })
            .collect())
    }

    pub fn get_available_printings(
        &self,
        card_requests: &[CardRequest],
    ) -> Result<HashMap<String, Vec<Printing>>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let unique_titles: HashSet<String> = card_requests
            .iter()
            .map(|r| normalize_title(&r.title))
            .collect();

        // build the "?1, ?2, ?3, ..." string for the in clause
        let placeholders = unique_titles
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

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

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(unique_titles.iter()))?;

        let mut map: HashMap<String, Vec<Printing>> = HashMap::new();
        while let Some(row) = rows.next()? {
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
            map.entry(normalized).or_default().push(printing);
        }

        let mut missing_titles = HashSet::new();
        for req in card_requests {
            let norm = normalize_title(&req.title);
            if !map.contains_key(&norm) && missing_titles.insert(norm) {
                eprintln!(
                    "Warning: No printings found for '{}' in your collections.",
                    req.title
                );
            }
        }

        if map.is_empty() && !card_requests.is_empty() {
            return Err("No printings found in your collections for any requested cards.".into());
        }

        Ok(map)
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
                match self.select_printing(request, printings) {
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
        &self,
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
