mod border_generator;
pub mod card_source;
pub mod card_store;
#[cfg(not(target_arch = "wasm32"))]
pub mod catalog;
#[cfg(not(target_arch = "wasm32"))]
pub mod collection_builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod collection_manager;
#[cfg(not(target_arch = "wasm32"))]
mod db_schema;
mod models;
pub mod mpc;
pub mod netrunnerdb;
pub mod pdf;
pub mod query;

use turso::Connection;

pub async fn setup_database(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute("PRAGMA foreign_keys = ON", ()).await?;

    #[cfg(not(target_arch = "wasm32"))]
    db_schema::create_app_schema(conn).await?;

    Ok(())
}
