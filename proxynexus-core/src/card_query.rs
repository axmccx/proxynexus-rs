use crate::card_source::{CardSource, Cardlist, SetName};
use crate::models::{CardRequest, Printing};
use dirs;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct CardQuery {
    app_db_path: PathBuf,
    collections_dir: PathBuf,
}

pub fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

impl CardSource for Cardlist {
    fn to_card_requests(&self) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let query = CardQuery::new()?;
        query.parse_cardlist_into_card_requests(&self.0)
    }
}

impl CardSource for SetName {
    fn to_card_requests(&self) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let query = CardQuery::new()?;
        query.get_card_requests_from_set_name(&self.0)
    }
}

impl CardQuery {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Could not find home directory")?;
        let proxynexus_dir = home.join(".proxynexus");
        let collections_dir = proxynexus_dir.join("collections");
        let app_db_path = proxynexus_dir.join("proxynexus.db");

        if !app_db_path.exists() {
            return Err("No ProxyNexus database found. Add a collection first.".into());
        }

        Ok(Self {
            app_db_path,
            collections_dir,
        })
    }

    fn parse_cardlist_into_card_requests(
        &self,
        text: &str,
    ) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let mut entries: Vec<(&str, u32, Option<String>, Option<String>)> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let (qty, rest) = self.parse_quantity(line);
            let (name, variant_pref, collection_pref) = self.parse_overrides(rest)?;

            entries.push((name, qty, variant_pref, collection_pref));
        }

        let names: Vec<&str> = entries.iter().map(|(name, _, _, _)| *name).collect();
        let codes = self.resolve_names_to_codes(&names)?;

        Ok(codes
            .into_iter()
            .zip(entries)
            .flat_map(|(code, (_, qty, variant, collection))| {
                std::iter::repeat(CardRequest {
                    code: code.clone(),
                    variant: variant.clone(),
                    collection: collection.clone(),
                })
                .take(qty as usize)
            })
            .collect())
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
    ) -> Result<(&'a str, Option<String>, Option<String>), Box<dyn std::error::Error>> {
        if let Some(bracket_start) = text.find('[') {
            let name = text[..bracket_start].trim();
            let bracket_end = text.find(']').ok_or("Unclosed bracket in card override")?;

            let override_text = text[bracket_start + 1..bracket_end]
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();

            if override_text.is_empty() {
                return Err("Empty override brackets".into());
            }

            // Parse variant:collection format
            let (variant_pref, collection_pref) =
                if let Some((v, c)) = override_text.split_once(':') {
                    let variant = if v.is_empty() {
                        None
                    } else {
                        Some(v.to_string())
                    };
                    let collection = if c.is_empty() {
                        None
                    } else {
                        Some(c.to_string())
                    };
                    (variant, collection)
                } else {
                    // Just variant, no colon
                    (Some(override_text), None)
                };

            Ok((name, variant_pref, collection_pref))
        } else {
            // No overrides
            Ok((text.trim(), None, None))
        }
    }

    fn resolve_names_to_codes(
        &self,
        names: &[&str],
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        let mut codes = Vec::new();
        let mut not_found = Vec::new();

        for name in names {
            let normalized = normalize_title(name);
            let result: Option<String> = conn
                .query_row(
                    "SELECT code FROM cards WHERE title_normalized = ?1",
                    params![normalized],
                    |row| row.get(0),
                )
                .optional()?;

            match result {
                Some(code) => codes.push(code),
                None => not_found.push(*name),
            }
        }

        if !not_found.is_empty() {
            return Err(format!("Could not resolve card titles: {}", not_found.join(", ")).into());
        }

        Ok(codes)
    }

    fn get_card_requests_from_set_name(
        &self,
        set_name: &str,
    ) -> Result<Vec<CardRequest>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;

        let mut stmt = conn.prepare(
            "SELECT code, quantity
            FROM cards
            WHERE set_name = ?1
            ORDER BY code",
        )?;

        let rows = stmt
            .query_map(params![set_name], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<(String, u32)>, _>>()?;

        if rows.is_empty() {
            return Err(format!("No cards found for set '{}'", set_name).into());
        }

        Ok(rows
            .into_iter()
            .flat_map(|(code, qty)| {
                std::iter::repeat(CardRequest {
                    code: code.clone(),
                    variant: None,
                    collection: None,
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

        let unique_codes: Vec<String> = card_requests
            .iter()
            .map(|item| item.code.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // build the "?1, ?2, ?3, ..." string for the in clause
        let placeholders: String = unique_codes
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            "SELECT p.card_code, p.variant, p.file_path, col.name, c.side
             FROM printings p
             JOIN cards c ON p.card_code = c.code
             JOIN collections col ON p.collection_id = col.id
             WHERE p.card_code IN ({})
             ORDER BY
                 c.release_date DESC NULLS LAST,
                 col.added_date DESC",
            placeholders
        );

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(unique_codes.iter()))?;

        let mut map: HashMap<String, Vec<Printing>> = HashMap::new();

        while let Some(row) = rows.next()? {
            let card_code: String = row.get(0)?;
            let variant: String = row.get(1)?;
            let file_path: String = row.get(2)?;
            let collection: String = row.get(3)?;
            let side: String = row.get(4)?;

            map.entry(card_code.clone()).or_default().push(Printing {
                card_code,
                variant,
                file_path,
                collection,
                side,
            });
        }

        let missing_codes: Vec<String> = unique_codes
            .iter()
            .filter(|code| !map.contains_key(*code))
            .cloned()
            .collect();

        if !missing_codes.is_empty() {
            eprintln!(
                "Warning: {} card(s) not found in collections:",
                missing_codes.len()
            );
            for code in &missing_codes {
                eprintln!("  - {}", code);
            }
        }

        Ok(map)
    }

    pub fn resolve_printings(
        &self,
        requests: &[CardRequest],
        available: &HashMap<String, Vec<Printing>>,
    ) -> Result<Vec<Printing>, Box<dyn std::error::Error>> {
        requests
            .iter()
            .filter_map(|request| {
                let printings = available.get(&request.code)?;
                Some(self.select_printing(request, printings))
            })
            .collect()
    }

    fn select_printing(
        &self,
        request: &CardRequest,
        printings: &[Printing],
    ) -> Result<Printing, Box<dyn std::error::Error>> {
        let default_collection = self.get_default_collection()?;

        // Try exact match: variant + collection
        if let (Some(variant), Some(collection)) = (&request.variant, &request.collection) {
            if let Some(p) = printings
                .iter()
                .find(|p| &p.variant == variant && &p.collection == collection)
            {
                return Ok(p.clone());
            }
            return Err(format!(
                "Printing not found: variant '{}' in collection '{}' for card {}",
                variant, collection, request.code
            )
            .into());
        }

        // Try variant only (check default collection first, then any)
        if let Some(variant) = &request.variant {
            if let Some(def_col) = default_collection {
                if let Some(p) = printings
                    .iter()
                    .find(|p| &p.variant == variant && &p.collection == &def_col)
                {
                    return Ok(p.clone());
                }
            }

            if let Some(p) = printings.iter().find(|p| &p.variant == variant) {
                return Ok(p.clone());
            }

            return Err(
                format!("Variant '{}' not found for card {}", variant, request.code).into(),
            );
        }

        if let Some(collection) = &request.collection {
            if let Some(p) = printings
                .iter()
                .find(|p| &p.collection == collection && p.variant == "original")
            {
                return Ok(p.clone());
            }

            if let Some(p) = printings.iter().find(|p| &p.collection == collection) {
                return Ok(p.clone());
            }

            return Err(format!(
                "Collection '{}' not found for card {}",
                collection, request.code
            )
            .into());
        }

        // No preferences - use defaults
        if let Some(def_col) = default_collection {
            if let Some(p) = printings
                .iter()
                .find(|p| &p.collection == &def_col && p.variant == "original")
            {
                return Ok(p.clone());
            }
        }

        if let Some(p) = printings.iter().find(|p| p.variant == "original") {
            return Ok(p.clone());
        }

        Ok(printings[0].clone())
    }

    fn get_default_collection(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;

        let result: Option<String> = conn
            .query_row(
                "SELECT name FROM collections ORDER BY added_date DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        Ok(result)
    }

    pub fn resolve_printing_to_full_path(
        &self,
        printing: &Printing,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let path = self.collections_dir.join(&printing.file_path);
        if !path.exists() {
            return Err(format!(
                "Image file not found: {} (printing: {} {})",
                path.display(),
                printing.card_code,
                printing.variant
            )
            .into());
        }
        Ok(path)
    }
}
