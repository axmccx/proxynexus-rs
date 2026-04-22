use crate::db_storage::{DbStorage, quote_sql_string};
use crate::error::Result;
use crate::games::l5r::api;
use crate::games::l5r::models::{Card, Pack};
use futures::join;
use gluesql::FromGlueRow;
use gluesql::core::row_conversion::SelectExt;
use tracing::{error, info};

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
            info!("Seeding L5R card catalog from EmeraldDB API...");
            match self.update_from_api().await {
                Ok(_) => info!("L5R card catalog seeded successfully!"),
                Err(e) => {
                    error!("Failed to fetch L5R catalog from EmeraldDB: {}", e);
                }
            }
        }

        Ok(())
    }

    pub async fn update_from_api(&mut self) -> Result<()> {
        let (cards_result, packs_result) = join!(api::fetch_cards(), api::fetch_packs());
        let cards = cards_result?;
        let packs = packs_result?;

        self.write_to_db(cards, packs).await
    }

    async fn write_to_db(&mut self, cards: Vec<Card>, packs: Vec<Pack>) -> Result<()> {
        self.db.execute("BEGIN").await?;

        let tx_result: Result<()> = async {
            self.db.execute("DELETE FROM l5r_card_versions").await?;
            self.db.execute("DELETE FROM l5r_cards").await?;
            self.db.execute("DELETE FROM l5r_packs").await?;

            for pack in packs {
                let released_at = pack
                    .released_at
                    .as_ref()
                    .map_or("NULL".to_string(), |v| quote_sql_string(v));
                let q = format!(
                    "INSERT INTO l5r_packs (id, name, released_at, cycle_id) VALUES ({}, {}, {}, {})",
                    quote_sql_string(&pack.id),
                    quote_sql_string(&pack.name),
                    released_at,
                    quote_sql_string(&pack.cycle_id),
                );
                self.db.execute(&q).await?;
            }

            for card in cards {
                let name_extra = card
                    .name_extra
                    .as_ref()
                    .map_or("NULL".to_string(), |v| quote_sql_string(v));
                let q = format!(
                    "INSERT INTO l5r_cards (id, name, name_extra, side, card_type) VALUES ({}, {}, {}, {}, {})",
                    quote_sql_string(&card.id),
                    quote_sql_string(&card.name),
                    name_extra,
                    quote_sql_string(&card.side),
                    quote_sql_string(&card.type_),
                );
                self.db.execute(&q).await?;

                for version in card.versions {
                    let image_url = version
                        .image_url
                        .as_ref()
                        .map_or("NULL".to_string(), |v| quote_sql_string(v));
                    let q = format!(
                        "INSERT INTO l5r_card_versions (card_id, pack_id, image_url, quantity) VALUES ({}, {}, {}, {})",
                        quote_sql_string(&version.card_id),
                        quote_sql_string(&version.pack_id),
                        image_url,
                        version.quantity,
                    );
                    self.db.execute(&q).await?;
                }
            }

            Ok(())
        }
        .await;

        match tx_result {
            Ok(()) => {
                self.db.execute("COMMIT").await?;
                Ok(())
            }
            Err(e) => {
                let _ = self.db.execute("ROLLBACK").await;
                Err(e)
            }
        }
    }

    async fn get_card_count(&mut self) -> Result<i64> {
        let payloads = self
            .db
            .execute("SELECT COUNT(*) AS count FROM l5r_cards")
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
