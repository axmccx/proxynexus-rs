use crate::card_store::normalize_title;
use crate::models::{Card, Pack};
use serde::Deserialize;
use std::path::PathBuf;
use turso::{Connection, params};

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

pub struct Catalog {
    conn: Connection,
}

impl Catalog {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub async fn seed_if_empty(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let count = self.get_card_count().await?;

        if count == 0 {
            println!("Seeding card catalog...");
            self.seed_from_json(CARDS_JSON, PACKS_JSON).await?;
            println!("Card catalog seeded successfully!");
        }

        Ok(())
    }

    pub async fn update_from_api(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let cards_json = reqwest::get("https://netrunnerdb.com/api/2.0/public/cards")
            .await?
            .text()
            .await?;

        let packs_json = reqwest::get("https://netrunnerdb.com/api/2.0/public/packs")
            .await?
            .text()
            .await?;

        self.seed_from_json(&cards_json, &packs_json).await?;

        Ok(())
    }

    pub async fn update_catalog_from_files(
        &mut self,
        cards_path: &PathBuf,
        packs_path: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cards_json = std::fs::read_to_string(cards_path)?;
        let packs_json = std::fs::read_to_string(packs_path)?;

        self.seed_from_json(&cards_json, &packs_json).await?;

        Ok(())
    }

    async fn seed_from_json(
        &mut self,
        cards_json: &str,
        packs_json: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cards_response: CardsResponse = serde_json::from_str(cards_json)?;
        let packs_response: PacksResponse = serde_json::from_str(packs_json)?;

        self.conn.execute("PRAGMA foreign_keys = OFF", ()).await?;

        let tx = self.conn.transaction().await?;

        tx.execute("DELETE FROM cards", ()).await?;
        tx.execute("DELETE FROM packs", ()).await?;

        for pack in packs_response.data {
            tx.execute(
                "INSERT INTO packs (code, name, date_release) VALUES (?1, ?2, ?3)",
                params![pack.code, pack.name, pack.date_release],
            )
            .await?;
        }

        for card in cards_response.data {
            tx.execute(
                "INSERT INTO cards (code, title, title_normalized, pack_code, side, quantity)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    card.code,
                    card.title.clone(),
                    normalize_title(&card.title),
                    card.pack_code,
                    card.side_code,
                    card.quantity,
                ],
            )
            .await?;
        }

        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('catalog_version', ?1)",
            params![cards_response.last_updated],
        )
        .await?;

        tx.commit().await?;

        self.conn.execute("PRAGMA foreign_keys = ON", ()).await?;

        Ok(())
    }

    pub async fn get_info(&self) -> Result<String, Box<dyn std::error::Error>> {
        let count = self.get_card_count().await?;

        let row = self
            .conn
            .query("SELECT value FROM meta WHERE key = 'catalog_version'", ())
            .await?
            .next()
            .await?;

        let last_updated = match row {
            Some(r) => r.get(0)?,
            None => "Unknown (bundled snapshot)".to_string(),
        };

        let info = format!(
            "Card Catalog Info:\n\
         - Cards: {}\n\
         - Last Updated: {}",
            count, last_updated
        );

        Ok(info)
    }

    async fn get_card_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let row = self
            .conn
            .query("SELECT COUNT(*) FROM cards", ())
            .await?
            .next()
            .await?;

        Ok(match row {
            Some(r) => r.get(0)?,
            None => 0,
        })
    }
}
