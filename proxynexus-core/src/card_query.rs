use crate::models::Printing;
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

    pub fn parse_cardlist_text(
        &self,
        text: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut entries: Vec<(&str, u32)> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let (qty, name) = if let Some((qty_str, card_name)) = line
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
            };

            entries.push((name, qty));
        }

        let names: Vec<&str> = entries.iter().map(|(name, _)| *name).collect();
        let codes = self.resolve_names_to_codes(&names)?;

        Ok(codes
            .into_iter()
            .zip(entries.into_iter())
            .flat_map(|(code, (_, qty))| std::iter::repeat(code).take(qty as usize))
            .collect())
    }

    pub fn get_available_sets(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;

        let mut stmt = conn.prepare("SELECT DISTINCT set_name FROM cards ORDER BY set_name")?;

        let results = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(results)
    }

    pub fn get_set_cards(&self, set_name: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;

        let mut stmt = conn.prepare(
            "SELECT code, quantity FROM cards
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
            .flat_map(|(code, qty)| std::iter::repeat(code).take(qty as usize))
            .collect())
    }

    pub fn get_available_printings(
        &self,
        card_codes: &[String],
    ) -> Result<HashMap<String, Vec<Printing>>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;

        // build the "?1, ?2, ?3, ..." string for the in clause
        let placeholders: String = card_codes
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
        let mut rows = stmt.query(rusqlite::params_from_iter(card_codes.iter()))?;

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

        Ok(map)
    }

    pub fn select_default_printings(
        &self,
        available: &HashMap<String, Vec<Printing>>,
    ) -> Result<HashMap<String, Printing>, Box<dyn std::error::Error>> {
        let mut selected = HashMap::new();

        for (code, printings) in available {
            let chosen = printings
                .iter()
                .find(|p| p.variant == "original")
                .unwrap_or(&printings[0])
                .clone();

            selected.insert(code.clone(), chosen);
        }

        Ok(selected)
    }

    fn resolve_printing_to_full_path(
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

    pub fn make_full_image_paths(
        &self,
        card_codes: &[String],
        selected: &HashMap<String, Printing>,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        card_codes
            .iter()
            .map(|code| {
                let printing = selected
                    .get(code)
                    .ok_or_else(|| format!("No printing selected for code: {}", code))?;

                self.resolve_printing_to_full_path(printing)
            })
            .collect()
    }

    pub fn make_printings_list(
        &self,
        card_codes: &[String],
        selected: &HashMap<String, Printing>,
    ) -> Result<Vec<Printing>, Box<dyn std::error::Error>> {
        card_codes
            .iter()
            .map(|code| {
                selected
                    .get(code)
                    .ok_or_else(|| format!("No printing selected for code: {}", code).into())
                    .map(|p| p.clone())
            })
            .collect()
    }
}
