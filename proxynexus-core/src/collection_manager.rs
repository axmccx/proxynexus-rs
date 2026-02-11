use crate::collection::{CardMetadata, Manifest};
use crate::db::app_schema;
use dirs;
use rusqlite::{Connection, OptionalExtension, params};
use std::fs;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub struct CollectionManager {
    app_db_path: PathBuf,
    collections_dir: PathBuf,
}

impl CollectionManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Could not find home directory")?;

        let proxynexus_dir = home.join(".proxynexus");
        let collections_dir = proxynexus_dir.join("collections");
        let app_db_path = proxynexus_dir.join("proxynexus.db");

        fs::create_dir_all(&collections_dir)?;

        let conn = Connection::open(&app_db_path)?;
        app_schema::create_app_schema(&conn)?;

        Ok(Self {
            app_db_path,
            collections_dir,
        })
    }

    pub fn add_collection(&self, pnx_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if !pnx_path.exists() {
            return Err(format!("File not found: {:?}", pnx_path).into());
        }

        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        let file = fs::File::open(pnx_path)?;
        let mut archive = ZipArchive::new(file)?;
        archive.extract(temp_path)?;

        let manifest_path = temp_path.join("manifest.toml");
        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: Manifest = toml::from_str(&manifest_content)?;

        let collection_name = pnx_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid filename")?
            .to_string();

        println!(
            "Adding collection: {} (v{}, {})",
            collection_name, manifest.version, manifest.language
        );

        let app_conn = Connection::open(&self.app_db_path)?;

        let existing: Option<i64> = app_conn
            .query_row(
                "SELECT id FROM collections WHERE name = ?1",
                params![&collection_name],
                |row| row.get(0),
            )
            .optional()?;

        if existing.is_some() {
            return Err(format!("Collection '{}' has already been added.", collection_name).into());
        }

        app_conn.execute(
            "INSERT INTO collections (name, version, language, added_date)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![&collection_name, &manifest.version, &manifest.language,],
        )?;

        let collection_id: i64 = app_conn.last_insert_rowid();

        let collection_db_path = temp_path.join("index.db");
        let collection_conn = Connection::open(&collection_db_path)?;

        let mut card_stmt = collection_conn.prepare(
            "SELECT code, title, set_code, set_name, release_date, side, quantity FROM cards",
        )?;

        let cards = card_stmt.query_map([], |row| {
            Ok(CardMetadata {
                code: row.get(0)?,
                title: row.get(1)?,
                set_code: row.get(2)?,
                set_name: row.get(3)?,
                release_date: row.get(4)?,
                side: row.get(5)?,
                quantity: row.get(6)?,
            })
        })?;

        let mut cards_added = 0;
        let mut existing_cards = 0;

        for card_result in cards {
            let card = card_result?;

            match app_conn.execute(
                "INSERT INTO cards (code, title, set_code, set_name, release_date, side, quantity, first_seen_collection_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &card.code,
                    &card.title,
                    &card.set_code,
                    &card.set_name,
                    &card.release_date,
                    &card.side,
                    &card.quantity,
                    collection_id,
                ],
            ) {
                Ok(_) => cards_added += 1,
                Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                    {
                        // Card already exists - validate it matches
                        let existing: (String, String, u32) = app_conn.query_row(
                            "SELECT title, set_code, quantity FROM cards WHERE code = ?1",
                            params![&card.code],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )?;

                        if existing.0 != card.title || existing.2 != card.quantity {
                            return Err(format!(
                                "Card mismatch for {}: existing='{}' (qty {}), new='{}' (qty {})",
                                card.code, existing.0, existing.2, card.title, card.quantity
                            ).into());
                        }

                        existing_cards += 1;
                    }
                Err(e) => return Err(e.into()),
            }
        }

        println!("Added {} new cards", cards_added);

        if existing_cards > 0 {
            println!("{} cards already existed", existing_cards);
        }

        let mut printing_stmt =
            collection_conn.prepare("SELECT card_code, variant, file_name FROM printings")?;

        let printings = printing_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // card_code
                row.get::<_, String>(1)?, // variant
                row.get::<_, String>(2)?, // file_name
            ))
        })?;

        let mut printings_added = 0;

        for printing_result in printings {
            let (card_code, variant, file_name) = printing_result?;

            let file_path = format!("{}/{}", collection_name, file_name);

            app_conn.execute(
                "INSERT INTO printings (collection_id, card_code, variant, file_path)
                 VALUES (?1, ?2, ?3, ?4)",
                params![collection_id, &card_code, &variant, &file_path,],
            )?;
            printings_added += 1;
        }

        println!("Added {} printings", printings_added);

        let collection_dir = self.collections_dir.join(&collection_name);
        fs::create_dir_all(&collection_dir)?;

        let src_images = temp_path.join("images");

        for entry in fs::read_dir(src_images)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let dst_path = collection_dir.join(entry.file_name());
                fs::copy(entry.path(), dst_path)?;
            }
        }

        println!("Collection '{}' added successfully!", collection_name);

        Ok(())
    }

    pub fn get_collections(
        &self,
    ) -> Result<Vec<(String, String, String)>, Box<dyn std::error::Error>> {
        let app_conn = Connection::open(&self.app_db_path)?;

        let mut stmt =
            app_conn.prepare("SELECT name, version, language FROM collections ORDER BY name")?;

        let collections = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(collections)
    }

    pub fn remove_collection(
        &self,
        collection_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut app_conn = Connection::open(&self.app_db_path)?;

        let collection_id: i64 = app_conn
            .query_row(
                "SELECT id FROM collections WHERE name = ?",
                [collection_name],
                |row| row.get(0),
            )
            .map_err(|_| format!("Collection '{}' not found", collection_name))?;

        let tx = app_conn.transaction()?;

        tx.execute(
            "DELETE FROM printings WHERE collection_id = ?",
            [collection_id],
        )?;

        tx.execute(
            "DELETE FROM cards WHERE code NOT IN (SELECT DISTINCT card_code FROM printings)",
            [],
        )?;

        tx.execute("DELETE FROM collections WHERE id = ?", [collection_id])?;

        tx.commit()?;

        let collection_dir = self.collections_dir.join(collection_name);
        if collection_dir.exists() {
            fs::remove_dir_all(&collection_dir)?;
        }

        Ok(())
    }
}
