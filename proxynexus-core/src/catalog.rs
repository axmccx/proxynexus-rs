use crate::card_store::normalize_title;
use crate::db_storage::{DbStorage, quote_sql_string};
use crate::error::Result;
use crate::models::{NrdbCard, NrdbCardSet, NrdbPrinting, NrdbResponse};
use gluesql::FromGlueRow;
use gluesql::core::row_conversion::SelectExt;
use tracing::{error, info};

#[derive(FromGlueRow)]
struct MetaRow {
    value: String,
}

#[derive(FromGlueRow)]
struct CountRow {
    count: i64,
}

pub struct Catalog<'a> {
    db: &'a mut DbStorage,
}

impl<'a> Catalog<'a> {
    pub fn new(db: &'a mut DbStorage) -> Self {
        Self { db }
    }

    pub async fn seed_if_empty(&mut self) -> Result<()> {
        let count = self.get_card_count().await?;

        if count == 0 {
            info!("Seeding card catalog from NetrunnerDB API...");
            match self.update_from_api().await {
                Ok(_) => info!("Card catalog seeded successfully!"),
                Err(e) => {
                    error!("Failed to fetch catalog from NetrunnerDB: {}", e);
                    error!("Check your internet connection.");
                }
            }
        }

        Ok(())
    }

    async fn fetch_v3_endpoint<T: for<'de> serde::Deserialize<'de>>(
        &self,
        url: &str,
    ) -> Result<Vec<T>> {
        let mut all_data = Vec::new();
        let mut current_url = Some(url.to_string());

        while let Some(u) = current_url {
            let json_str = reqwest::get(&u).await?.text().await?;

            let response: NrdbResponse<T> = serde_json::from_str(&json_str)?;
            all_data.extend(response.data);

            current_url = response.links.and_then(|l| l.next);
        }

        Ok(all_data)
    }

    pub async fn update_from_api(&mut self) -> Result<()> {
        let base_url = "https://api-preview.netrunnerdb.com/api/v3/public";

        let sets: Vec<NrdbCardSet> = self
            .fetch_v3_endpoint(&format!("{}/card_sets?page[size]=1000", base_url))
            .await?;
        let cards: Vec<NrdbCard> = self
            .fetch_v3_endpoint(&format!("{}/cards?page[size]=1000", base_url))
            .await?;
        let printings: Vec<NrdbPrinting> = self
            .fetch_v3_endpoint(&format!("{}/printings?page[size]=1000", base_url))
            .await?;

        self.seed_catalog(&sets, &cards, &printings).await?;

        Ok(())
    }

    async fn seed_catalog(
        &mut self,
        sets: &[NrdbCardSet],
        cards: &[NrdbCard],
        printings: &[NrdbPrinting],
    ) -> Result<()> {
        self.db.execute("BEGIN").await?;

        self.db.execute("DELETE FROM cards").await?;
        self.db.execute("DELETE FROM packs").await?;
        self.db.execute("DELETE FROM card_versions").await?;

        let game_id = quote_sql_string("netrunner");

        for set in sets {
            let date = set
                .attributes
                .date_release
                .as_ref()
                .map_or("NULL".to_string(), |d| quote_sql_string(d));
            let q = format!(
                "INSERT INTO packs (id, game_id, name, date_release) VALUES ({}, {}, {}, {})",
                quote_sql_string(&set.id),
                game_id,
                quote_sql_string(&set.attributes.name),
                date
            );
            self.db.execute(&q).await?;
        }

        for card in cards {
            let q = format!(
                "INSERT INTO cards (id, game_id, title, title_normalized, side) VALUES ({}, {}, {}, {}, {})",
                quote_sql_string(&card.id),
                game_id,
                quote_sql_string(&card.attributes.title),
                quote_sql_string(&normalize_title(&card.attributes.title)),
                quote_sql_string(&card.attributes.side_id)
            );
            self.db.execute(&q).await?;
        }

        for printing in printings {
            let synthesized_id = format!(
                "{}_{}",
                printing.attributes.card_id, printing.attributes.card_set_id
            );
            let q = format!(
                "INSERT INTO card_versions (id, card_id, pack_id, quantity) VALUES ({}, {}, {}, {})",
                quote_sql_string(&synthesized_id),
                quote_sql_string(&printing.attributes.card_id),
                quote_sql_string(&printing.attributes.card_set_id),
                printing.attributes.quantity
            );
            self.db.execute(&q).await?;
        }

        self.db
            .execute("DELETE FROM meta WHERE key = 'catalog_version'")
            .await?;
        let q = format!(
            "INSERT INTO meta (key, value) VALUES ('catalog_version', {})",
            quote_sql_string(&chrono::Utc::now().to_rfc3339())
        );
        self.db.execute(&q).await?;

        self.db.execute("COMMIT").await?;

        Ok(())
    }

    pub async fn get_info(&mut self) -> Result<String> {
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

    async fn get_card_count(&mut self) -> Result<i64> {
        let payloads = self
            .db
            .execute("SELECT COUNT(*) AS count FROM cards WHERE game_id = 'netrunner'")
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
