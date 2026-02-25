use crate::models::Manifest;
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

        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM collections WHERE name = ?1",
                params![&collection_name],
                |row| row.get(0),
            )
            .optional()?;

        if existing.is_some() {
            return Err(format!("Collection '{}' has already been added.", collection_name).into());
        }

        conn.execute(
            "INSERT INTO collections (name, version, language, added_date)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![&collection_name, &manifest.version, &manifest.language,],
        )?;

        let collection_id: i64 = conn.last_insert_rowid();

        let collection_dir = self.collections_dir.join(&collection_name);
        fs::create_dir_all(&collection_dir)?;

        let src_images = temp_path.join("images");

        let mut printings_added = 0;

        for entry in fs::read_dir(&src_images)? {
            let entry = entry?;
            let path = entry.path();

            let (card_code, variant) = match self.parse_filename(&path) {
                Some(parsed) => parsed,
                None => continue,
            };

            let file_name = path.file_name().unwrap().to_string_lossy();
            let file_path = format!("{}/{}", collection_name, file_name);

            conn.execute(
                "INSERT INTO printings (collection_id, card_code, variant, file_path)
                 VALUES (?1, ?2, ?3, ?4)",
                params![collection_id, &card_code, &variant, &file_path,],
            )?;

            let dst_path = collection_dir.join(path.file_name().unwrap());
            fs::copy(entry.path(), dst_path)?;

            printings_added += 1;
        }

        println!("Added {} printings", printings_added);
        println!("Collection '{}' added successfully!", collection_name);

        Ok(())
    }

    fn parse_filename(&self, path: &Path) -> Option<(String, String)> {
        let stem = path.file_stem()?.to_str()?;

        let (code, variant) = if let Some((c, v)) = stem.split_once('_') {
            (c, v)
        } else {
            (stem, "original")
        };

        if !code.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        Some((code.to_string(), variant.to_string()))
    }

    pub fn get_collections(
        &self,
    ) -> Result<Vec<(String, String, String)>, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let mut stmt =
            conn.prepare("SELECT name, version, language FROM collections ORDER BY name")?;

        let collections = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(collections)
    }

    pub fn collection_exists(&self, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.app_db_path)?;
        let count: u32 = conn.query_row(
            "SELECT COUNT(*) FROM collections WHERE name = ?1",
            [name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn remove_collection(
        &self,
        collection_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open(&self.app_db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let collection_id: i64 = conn
            .query_row(
                "SELECT id FROM collections WHERE name = ?",
                [collection_name],
                |row| row.get(0),
            )
            .map_err(|_| format!("Collection '{}' not found", collection_name))?;

        let tx = conn.transaction()?;

        tx.execute(
            "DELETE FROM printings WHERE collection_id = ?",
            [collection_id],
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
