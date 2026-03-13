use crate::card_store::normalize_title;
use crate::db_storage::{DbStorage, quote_sql_string};
use crate::models::{Card, Pack};
use gluesql::FromGlueRow;
use gluesql::core::row_conversion::SelectExt;
use gluesql::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{error, info};

#[derive(FromGlueRow)]
struct MetaRow {
    value: String,
}

#[derive(FromGlueRow)]
struct CountRow {
    count: i64,
}

#[derive(Debug, Deserialize)]
struct CardsResponse {
    data: Vec<Card>,
    last_updated: String,
}

#[derive(Debug, Deserialize)]
struct PacksResponse {
    data: Vec<Pack>,
}

pub struct Catalog<'a> {
    db: &'a mut DbStorage,
}

impl<'a> Catalog<'a> {
    pub fn new(db: &'a mut DbStorage) -> Self {
        Self { db }
    }

    pub async fn seed_if_empty(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let count = self.get_card_count().await?;

        if count == 0 {
            info!("Seeding card catalog from NetrunnerDB API...");
            match self.update_from_api().await {
                Ok(_) => info!("Card catalog seeded successfully!"),
                Err(e) => {
                    error!("Failed to fetch catalog from NetrunnerDB: {}", e);
                    error!(
                        "If you do not have internet access, you can download the data manually:"
                    );
                    error!("  curl -o cards.json https://netrunnerdb.com/api/2.0/public/cards");
                    error!("  curl -o packs.json https://netrunnerdb.com/api/2.0/public/packs");
                    error!("Then use the CLI to import them:");
                    error!("  proxynexus-cli catalog import cards.json packs.json");
                }
            }
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

        self.db.execute("BEGIN").await?;

        self.db.execute("DELETE FROM cards").await?;
        self.db.execute("DELETE FROM packs").await?;

        for pack in packs_response.data {
            let date = pack
                .date_release
                .map_or("NULL".to_string(), |d| quote_sql_string(&d));
            let q = format!(
                "INSERT INTO packs (code, name, date_release) VALUES ({}, {}, {})",
                quote_sql_string(&pack.code),
                quote_sql_string(&pack.name),
                date
            );
            self.db.execute(&q).await?;
        }

        for card in cards_response.data {
            let q = format!(
                "INSERT INTO cards (code, title, title_normalized, pack_code, side, quantity) VALUES ({}, {}, {}, {}, {}, {})",
                quote_sql_string(&card.code),
                quote_sql_string(&card.title),
                quote_sql_string(&normalize_title(&card.title)),
                quote_sql_string(&card.pack_code),
                quote_sql_string(&card.side_code),
                card.quantity
            );
            self.db.execute(&q).await?;
        }

        self.db
            .execute("DELETE FROM meta WHERE key = 'catalog_version'")
            .await?;
        let q = format!(
            "INSERT INTO meta (key, value) VALUES ('catalog_version', {})",
            quote_sql_string(&cards_response.last_updated)
        );
        self.db.execute(&q).await?;

        self.db.execute("COMMIT").await?;

        Ok(())
    }

    pub async fn get_info(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let count = self.get_card_count().await?;

        let payloads = self
            .db
            .execute("SELECT value FROM meta WHERE key = 'catalog_version'")
            .await?;

        let last_updated = match payloads.into_iter().next() {
            Some(p) => p
                .rows_as::<MetaRow>()?
                .into_iter()
                .next()
                .map(|row| row.value)
                .unwrap_or_else(|| "Unknown (bundled snapshot)".to_string()),
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

    async fn get_card_count(&mut self) -> Result<i64, Box<dyn std::error::Error>> {
        let payloads = self
            .db
            .execute("SELECT COUNT(*) AS count FROM cards")
            .await?;

        let count = match payloads.into_iter().next() {
            Some(p) => p
                .rows_as::<CountRow>()?
                .into_iter()
                .next()
                .map(|row| row.count)
                .unwrap_or(0),
            None => 0,
        };

        Ok(count)
    }
}
