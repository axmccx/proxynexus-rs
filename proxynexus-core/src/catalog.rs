use crate::db_schema;
use crate::models::{Card, Pack};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use std::path::PathBuf;

const CARDS_JSON: &str = include_str!("../data/netrunnerdb_cards.json");
const PACKS_JSON: &str = include_str!("../data/netrunnerdb_packs.json");

#[derive(Debug, Deserialize)]
struct CardsResponse {
    data: Vec<Card>,
    last_updated: String,
}

#[derive(Debug, Deserialize)]
struct PacksResponse {
    data: Vec<Pack>,
}

pub fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

pub struct Catalog {
    app_db_path: PathBuf,
}

impl Catalog {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Could not find home directory")?;
        let proxynexus_dir = home.join(".proxynexus");
        std::fs::create_dir_all(&proxynexus_dir)?;

        let app_db_path = proxynexus_dir.join("proxynexus.db");

        let conn = Connection::open(&app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        db_schema::create_app_schema(&conn)?;

        Ok(Self { app_db_path })
    }

    pub fn seed_if_empty(&self) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM cards", [], |row| row.get(0))?;

        if count > 0 {
            return Ok(());
        }

        println!("Seeding card catalog...");
        self.seed_from_json(CARDS_JSON, PACKS_JSON)?;
        println!("Card catalog seeded successfully!");

        Ok(())
    }

    pub fn update_from_api(&self) -> Result<(), Box<dyn std::error::Error>> {
        let cards_json =
            reqwest::blocking::get("https://netrunnerdb.com/api/2.0/public/cards")?.text()?;

        let packs_json =
            reqwest::blocking::get("https://netrunnerdb.com/api/2.0/public/packs")?.text()?;

        self.seed_from_json(&cards_json, &packs_json)?;

        Ok(())
    }

    pub fn update_catalog_from_files(
        &self,
        cards_path: &PathBuf,
        packs_path: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cards_json = std::fs::read_to_string(cards_path)?;
        let packs_json = std::fs::read_to_string(packs_path)?;

        self.seed_from_json(&cards_json, &packs_json)?;

        Ok(())
    }

    pub fn get_info(&self) -> Result<String, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        let card_count: i64 = conn.query_row("SELECT COUNT(*) FROM cards", [], |row| row.get(0))?;

        let last_updated: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'catalog_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let info = format!(
            "Card Catalog Info:\n\
         - Cards: {}\n\
         - Last Updated: {}",
            card_count,
            last_updated.unwrap_or_else(|| "Unknown (bundled snapshot)".to_string())
        );

        Ok(info)
    }

    fn seed_from_json(
        &self,
        cards_json: &str,
        packs_json: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let cards_response: CardsResponse = serde_json::from_str(cards_json)?;
        let packs_response: PacksResponse = serde_json::from_str(packs_json)?;

        let tx = conn.transaction()?;

        tx.execute("DELETE FROM cards", [])?;
        tx.execute("DELETE FROM packs", [])?;

        for pack in packs_response.data {
            tx.execute(
                "INSERT INTO packs (code, name, date_release) VALUES (?1, ?2, ?3)",
                params![&pack.code, &pack.name, &pack.date_release],
            )?;
        }

        for card in cards_response.data {
            tx.execute(
                "INSERT INTO cards (code, title, title_normalized, pack_code, side, quantity)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    &card.code,
                    &card.title,
                    &normalize_title(&card.title),
                    &card.pack_code,
                    &card.side_code,
                    &card.quantity,
                ],
            )?;
        }

        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('catalog_version', ?1)",
            params![&cards_response.last_updated],
        )?;

        tx.commit()?;

        Ok(())
    }
}
