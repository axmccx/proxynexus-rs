use crate::error::{ProxyNexusError, Result};
use gluesql::FromGlueRow;
use gluesql::prelude::*;

#[derive(FromGlueRow)]
pub struct IdRow {
    pub id: i64,
}

#[derive(FromGlueRow)]
struct MetaDbRow {
    key: String,
    value: String,
}

#[derive(FromGlueRow)]
struct CollectionDbRow {
    id: i64,
    name: String,
    version: Option<String>,
    language: Option<String>,
    added_date: String,
    last_updated: Option<String>,
}

#[derive(FromGlueRow)]
struct PackDbRow {
    code: String,
    name: String,
    date_release: Option<String>,
}

#[derive(FromGlueRow)]
struct CardDbRow {
    code: String,
    title: String,
    title_normalized: String,
    pack_code: String,
    side: String,
    quantity: i64,
}

#[derive(FromGlueRow)]
struct PrintingDbRow {
    id: i64,
    collection_id: i64,
    card_code: String,
    variant: String,
    file_path: String,
    part: String,
}

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
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_sled(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let storage = SledStorage::new(
            path.as_ref()
                .to_str()
                .ok_or_else(|| ProxyNexusError::Internal("Invalid path".to_string()))?,
        )?;
        Ok(Self::Sled(Glue::new(storage)))
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_memory() -> Self {
        let storage = MemoryStorage::default();
        Self::Memory(Glue::new(storage))
    }

    pub async fn execute(&mut self, sql: &str) -> Result<Vec<Payload>> {
        let res = match self {
            #[cfg(target_arch = "wasm32")]
            DbStorage::Memory(glue) => glue.execute(sql).await,

            #[cfg(not(target_arch = "wasm32"))]
            DbStorage::Sled(glue) => glue.execute(sql).await,
        };
        res.map_err(ProxyNexusError::from)
    }

    pub async fn get_next_id(&mut self, table_name: &str) -> Result<i64> {
        let query = format!("SELECT id FROM {} ORDER BY id DESC LIMIT 1", table_name);
        let payloads = self.execute(&query).await?;

        let next_id = match payloads.into_iter().next() {
            Some(p) => p
                .rows_as::<IdRow>()?
                .into_iter()
                .next()
                .map(|row| row.id + 1)
                .unwrap_or(1),
            None => 1,
        };
        Ok(next_id)
    }

    pub async fn initialize_schema(&mut self) -> Result<()> {
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
                part TEXT NOT NULL
            );
            ",
        )
        .await?;

        self.execute(crate::games::l5r::schema::DDL).await?;

        Ok(())
    }

    pub async fn export_sql(&mut self, path: &std::path::Path) -> Result<()> {
        let mut sql = String::new();

        let meta_payloads = self.execute("SELECT * FROM meta").await?;
        if let Some(payload) = meta_payloads.into_iter().next() {
            let rows: Vec<MetaDbRow> = payload.rows_as()?;
            for chunk in rows.chunks(500) {
                sql.push_str("INSERT INTO meta (key, value) VALUES ");
                let values: Vec<String> = chunk
                    .iter()
                    .map(|row| {
                        format!(
                            "({}, {})",
                            quote_sql_string(&row.key),
                            quote_sql_string(&row.value)
                        )
                    })
                    .collect();
                sql.push_str(&values.join(", "));
                sql.push_str(";\n");
            }
        }

        let pack_payloads = self.execute("SELECT * FROM packs").await?;
        if let Some(payload) = pack_payloads.into_iter().next() {
            let rows: Vec<PackDbRow> = payload.rows_as()?;
            for chunk in rows.chunks(500) {
                sql.push_str("INSERT INTO packs (code, name, date_release) VALUES ");
                let values: Vec<String> = chunk
                    .iter()
                    .map(|row| {
                        let date = row
                            .date_release
                            .as_ref()
                            .map_or("NULL".to_string(), |d| quote_sql_string(d));
                        format!(
                            "({}, {}, {})",
                            quote_sql_string(&row.code),
                            quote_sql_string(&row.name),
                            date
                        )
                    })
                    .collect();
                sql.push_str(&values.join(", "));
                sql.push_str(";\n");
            }
        }

        let card_payloads = self.execute("SELECT * FROM cards").await?;
        if let Some(payload) = card_payloads.into_iter().next() {
            let rows: Vec<CardDbRow> = payload.rows_as()?;
            for chunk in rows.chunks(500) {
                sql.push_str("INSERT INTO cards (code, title, title_normalized, pack_code, side, quantity) VALUES ");
                let values: Vec<String> = chunk
                    .iter()
                    .map(|row| {
                        format!(
                            "({}, {}, {}, {}, {}, {})",
                            quote_sql_string(&row.code),
                            quote_sql_string(&row.title),
                            quote_sql_string(&row.title_normalized),
                            quote_sql_string(&row.pack_code),
                            quote_sql_string(&row.side),
                            row.quantity
                        )
                    })
                    .collect();
                sql.push_str(&values.join(", "));
                sql.push_str(";\n");
            }
        }

        let coll_payloads = self.execute("SELECT * FROM collections").await?;
        if let Some(payload) = coll_payloads.into_iter().next() {
            let rows: Vec<CollectionDbRow> = payload.rows_as()?;
            for chunk in rows.chunks(500) {
                sql.push_str("INSERT INTO collections (id, name, version, language, added_date, last_updated) VALUES ");
                let values: Vec<String> = chunk
                    .iter()
                    .map(|row| {
                        let version = row
                            .version
                            .as_ref()
                            .map_or("NULL".to_string(), |v| quote_sql_string(v));
                        let lang = row
                            .language
                            .as_ref()
                            .map_or("NULL".to_string(), |l| quote_sql_string(l));
                        let last_up = row
                            .last_updated
                            .as_ref()
                            .map_or("NULL".to_string(), |d| quote_sql_string(d));
                        format!(
                            "({}, {}, {}, {}, {}, {})",
                            row.id,
                            quote_sql_string(&row.name),
                            version,
                            lang,
                            quote_sql_string(&row.added_date),
                            last_up
                        )
                    })
                    .collect();
                sql.push_str(&values.join(", "));
                sql.push_str(";\n");
            }
        }

        let print_payloads = self.execute("SELECT * FROM printings").await?;
        if let Some(payload) = print_payloads.into_iter().next() {
            let rows: Vec<PrintingDbRow> = payload.rows_as()?;
            for chunk in rows.chunks(500) {
                sql.push_str("INSERT INTO printings (id, collection_id, card_code, variant, file_path, part) VALUES ");
                let values: Vec<String> = chunk
                    .iter()
                    .map(|row| {
                        format!(
                            "({}, {}, {}, {}, {}, {})",
                            row.id,
                            row.collection_id,
                            quote_sql_string(&row.card_code),
                            quote_sql_string(&row.variant),
                            quote_sql_string(&row.file_path),
                            quote_sql_string(&row.part)
                        )
                    })
                    .collect();
                sql.push_str(&values.join(", "));
                sql.push_str(";\n");
            }
        }

        std::fs::write(path, sql)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_sql_string() {
        assert_eq!(quote_sql_string("hello"), "'hello'");
        assert_eq!(quote_sql_string("hello'world"), "'hello''world'");
        assert_eq!(quote_sql_string(""), "''");
        assert_eq!(quote_sql_string("'"), "''''");
    }

    #[test]
    fn test_build_in_clause() {
        assert_eq!(build_in_clause(vec!["a", "b", "c"]), "'a', 'b', 'c'");
        assert_eq!(build_in_clause(vec!["O'Brian", "b"]), "'O''Brian', 'b'");
        let empty: Vec<&str> = vec![];
        assert_eq!(build_in_clause(empty), "");
    }
}
