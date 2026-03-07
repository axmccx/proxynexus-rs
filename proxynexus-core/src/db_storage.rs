use gluesql::prelude::*;

#[cfg(target_arch = "wasm32")]
use gluesql_memory_storage::MemoryStorage;

#[cfg(not(target_arch = "wasm32"))]
use gluesql_sled_storage::SledStorage;

pub enum DbStorage {
    #[cfg(target_arch = "wasm32")]
    Memory(Glue<MemoryStorage>),

    #[cfg(not(target_arch = "wasm32"))]
    Sled(Glue<SledStorage>),
}

impl DbStorage {
    pub async fn execute(&mut self, sql: &str) -> Result<Vec<Payload>, Error> {
        match self {
            #[cfg(target_arch = "wasm32")]
            DbStorage::Memory(glue) => glue.execute(sql).await,

            #[cfg(not(target_arch = "wasm32"))]
            DbStorage::Sled(glue) => glue.execute(sql).await,
        }
    }

    pub async fn initialize_schema(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.execute(
            "
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS collections (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                version TEXT,
                language TEXT,
                added_date TEXT NOT NULL,
                last_updated TEXT
            );

            CREATE TABLE IF NOT EXISTS packs (
                code TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                date_release TEXT
            );

            CREATE TABLE IF NOT EXISTS cards (
                code TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                title_normalized TEXT NOT NULL,
                pack_code TEXT NOT NULL,
                side TEXT NOT NULL,
                quantity INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS printings (
                id INTEGER PRIMARY KEY,
                collection_id INTEGER NOT NULL,
                card_code TEXT NOT NULL,
                variant TEXT NOT NULL,
                file_path TEXT NOT NULL,
                UNIQUE(collection_id, card_code, variant)
            );
            ",
        )
        .await?;

        Ok(())
    }
}

pub fn quote_sql_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

pub fn build_in_clause(items: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    items
        .into_iter()
        .map(|s| quote_sql_string(s.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}
