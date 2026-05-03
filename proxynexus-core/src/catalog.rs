use crate::db_storage::{DbStorage, quote_sql_string};
use crate::error::Result;
use crate::games::netrunner::adapter::NetrunnerAdapter;
use async_trait::async_trait;
use gluesql::FromGlueRow;
use gluesql::core::row_conversion::SelectExt;
use tracing::{error, info};

#[derive(FromGlueRow)]
struct CountRow {
    count: i64,
}

pub struct Pack {
    pub id: String,
    pub name: String,
    pub date_release: Option<String>,
}

pub struct Card {
    pub id: String,
    pub title: String,
    pub title_normalized: String,
    pub side: Option<String>,
}

pub struct CardVersion {
    pub card_id: String,
    pub pack_id: String,
    pub quantity: i64,
    pub position: Option<i64>,
}

pub struct Catalog {
    pub game_id: String,
    pub display_name: String,
    pub packs: Vec<Pack>,
    pub cards: Vec<Card>,
    pub card_versions: Vec<CardVersion>,
}

#[async_trait]
pub trait CatalogProvider: Send + Sync {
    fn game_id(&self) -> &'static str;

    fn game_name(&self) -> &'static str;

    async fn fetch_catalog(&self) -> Result<Catalog>;
}

pub struct CatalogManager<'a> {
    db: &'a mut DbStorage,
    adapters: Vec<Box<dyn CatalogProvider>>,
}

impl<'a> CatalogManager<'a> {
    pub fn new(db: &'a mut DbStorage) -> Self {
        let adapters: Vec<Box<dyn CatalogProvider>> = vec![Box::new(NetrunnerAdapter::new())];

        Self { db, adapters }
    }

    pub async fn seed_if_empty(&mut self) -> Result<()> {
        let count = self.get_card_count(None).await?;

        if count == 0 {
            info!("No card data found. Initializing local catalog database...");
            match self.update_from_api().await {
                Ok(_) => info!("Catalog initialization complete."),
                Err(e) => {
                    error!("Failed to fetch catalog: {}", e);
                    error!("Check your internet connection.");
                }
            }
        }

        Ok(())
    }

    pub async fn update_from_api(&mut self) -> Result<()> {
        let mut catalogs = Vec::new();

        for adapter in &self.adapters {
            info!("Synchronizing {} catalog...", adapter.game_name());

            match adapter.fetch_catalog().await {
                Ok(catalog) => {
                    catalogs.push(catalog);
                }
                Err(e) => {
                    error!("Failed to fetch {} catalog: {}", adapter.game_name(), e);
                    return Err(e);
                }
            }
        }

        self.db.execute("BEGIN").await?;

        self.db.execute("DELETE FROM card_versions").await?;
        self.db.execute("DELETE FROM cards").await?;
        self.db.execute("DELETE FROM packs").await?;
        self.db.execute("DELETE FROM games").await?;

        for catalog in catalogs {
            info!(
                "Applying {} updates ({} cards, {} packs, {} card versions) to local database...",
                catalog.display_name,
                catalog.cards.len(),
                catalog.packs.len(),
                catalog.card_versions.len()
            );
            self.seed_catalog(&catalog).await?;
        }

        self.db.execute("COMMIT").await?;

        info!("Catalog synchronization complete.");

        Ok(())
    }

    async fn seed_catalog(&mut self, catalog: &Catalog) -> Result<()> {
        let q_ins_game = format!(
            "INSERT INTO games (id, display_name) VALUES ({}, {})",
            quote_sql_string(&catalog.game_id),
            quote_sql_string(&catalog.display_name)
        );
        self.db.execute(&q_ins_game).await?;

        for set in &catalog.packs {
            let date = set
                .date_release
                .as_ref()
                .map_or("NULL".to_string(), |d| quote_sql_string(d));
            let q = format!(
                "INSERT INTO packs (id, game_id, name, date_release) VALUES ({}, {}, {}, {})",
                quote_sql_string(&set.id),
                quote_sql_string(&catalog.game_id),
                quote_sql_string(&set.name),
                date
            );
            self.db.execute(&q).await?;
        }

        for card in &catalog.cards {
            let side = card
                .side
                .as_ref()
                .map_or("NULL".to_string(), |s| quote_sql_string(s));
            let q = format!(
                "INSERT INTO cards (id, game_id, title, title_normalized, side) VALUES ({}, {}, {}, {}, {})",
                quote_sql_string(&card.id),
                quote_sql_string(&catalog.game_id),
                quote_sql_string(&card.title),
                quote_sql_string(&card.title_normalized),
                side
            );
            self.db.execute(&q).await?;
        }

        for card_version in &catalog.card_versions {
            let synthesized_id = format!("{}_{}", card_version.card_id, card_version.pack_id);
            let position_val = card_version
                .position
                .map_or("NULL".to_string(), |p| p.to_string());
            let q = format!(
                "INSERT INTO card_versions (id, card_id, pack_id, quantity, position) VALUES ({}, {}, {}, {}, {})",
                quote_sql_string(&synthesized_id),
                quote_sql_string(&card_version.card_id),
                quote_sql_string(&card_version.pack_id),
                card_version.quantity,
                position_val
            );
            self.db.execute(&q).await?;
        }

        Ok(())
    }

    pub async fn get_info(&mut self) -> Result<String> {
        let mut info = String::from("Card Catalog Info:\n");

        let adapter_info: Vec<(String, String)> = self
            .adapters
            .iter()
            .map(|a| (a.game_id().to_string(), a.game_name().to_string()))
            .collect();

        for (game_id, game_name) in adapter_info {
            let count = self.get_card_count(Some(&game_id)).await?;
            info.push_str(&format!(" - {}: {} logical cards\n", game_name, count));
        }

        Ok(info)
    }

    async fn get_card_count(&mut self, game_id: Option<&str>) -> Result<i64> {
        let query = if let Some(gid) = game_id {
            format!(
                "SELECT COUNT(*) AS count FROM cards WHERE game_id = {}",
                quote_sql_string(gid)
            )
        } else {
            "SELECT COUNT(*) AS count FROM cards".to_string()
        };

        let payloads = self.db.execute(&query).await?;

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
