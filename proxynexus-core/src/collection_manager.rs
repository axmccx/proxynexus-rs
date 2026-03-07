use crate::db_storage::{DbStorage, IdRow, quote_sql_string};
use crate::models::Manifest;
use gluesql::FromGlueRow;
use gluesql::core::row_conversion::SelectExt;
use gluesql::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(FromGlueRow)]
struct CollectionRow {
    name: String,
    version: Option<String>,
    language: Option<String>,
}

#[derive(FromGlueRow)]
struct CountRow {
    count: i64,
}

pub struct CollectionManager<'a> {
    collections_dir: PathBuf,
    db: &'a mut DbStorage,
}

impl<'a> CollectionManager<'a> {
    pub fn new(
        db: &'a mut DbStorage,
        collections_dir: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        fs::create_dir_all(&collections_dir)?;

        Ok(Self {
            collections_dir,
            db,
        })
    }

    pub async fn add_collection(
        &mut self,
        pnx_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        if self.collection_exists(&collection_name).await? {
            return Err(format!("Collection '{}' has already been added.", collection_name).into());
        }

        let next_coll_id = self.db.get_next_id("collections").await?;

        let added_date = chrono::Utc::now().to_rfc3339();

        let insert_coll_q = format!(
            "INSERT INTO collections (id, name, version, language, added_date) VALUES ({}, {}, {}, {}, '{}')",
            next_coll_id,
            quote_sql_string(&collection_name),
            quote_sql_string(&manifest.version),
            quote_sql_string(&manifest.language),
            added_date
        );
        self.db.execute(&insert_coll_q).await?;

        let collection_id = next_coll_id;

        let collection_dir = self.collections_dir.join(collection_name.clone());
        fs::create_dir_all(&collection_dir)?;

        let src_images = temp_path.join("images");

        self.db.execute("BEGIN").await?;

        let mut next_print_id = self.db.get_next_id("printings").await?;

        let tx_result: Result<i32, Box<dyn std::error::Error>> = async {
            let mut printings_added = 0;
            for entry in fs::read_dir(&src_images)? {
                let entry = entry?;
                let path = entry.path();

                let (card_code, variant) = match Self::parse_filename(&path) {
                    Some(parsed) => parsed,
                    None => continue,
                };

                let file_name = path.file_name().unwrap().to_string_lossy();
                let file_path = format!("{}/{}", collection_name, file_name);

                let insert_print_q = format!(
                    "INSERT INTO printings (id, collection_id, card_code, variant, file_path) VALUES ({}, {}, {}, {}, {})",
                    next_print_id,
                    collection_id,
                    quote_sql_string(&card_code),
                    quote_sql_string(&variant),
                    quote_sql_string(&file_path)
                );

                self.db.execute(&insert_print_q).await?;
                next_print_id += 1;

                let dst_path = collection_dir.join(path.file_name().unwrap());
                fs::copy(entry.path(), dst_path)?;

                printings_added += 1;
            }
            Ok(printings_added)
        }
        .await;

        let printings_added = match tx_result {
            Ok(count) => {
                self.db.execute("COMMIT").await?;
                count
            }
            Err(e) => {
                let _ = self.db.execute("ROLLBACK").await;
                return Err(e);
            }
        };

        println!("Added {} printings", printings_added);
        println!("Collection '{}' added successfully!", collection_name);

        Ok(())
    }

    fn parse_filename(path: &Path) -> Option<(String, String)> {
        let stem = path.file_stem()?.to_str()?;

        let (code, variant) = if let Some((c, v)) = stem.split_once('_') {
            (c, v.to_lowercase())
        } else {
            (stem, "original".to_string())
        };

        if !code.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        Some((code.to_string(), variant))
    }

    pub async fn get_collections(
        &mut self,
    ) -> Result<Vec<(String, String, String)>, Box<dyn std::error::Error>> {
        let payloads = self
            .db
            .execute("SELECT name, version, language FROM collections ORDER BY name")
            .await?;

        let rows = match payloads.into_iter().next() {
            Some(p) => p.rows_as::<CollectionRow>()?,
            None => return Ok(Vec::new()),
        };

        let results = rows
            .into_iter()
            .map(|row| {
                (
                    row.name,
                    row.version.unwrap_or_default(),
                    row.language.unwrap_or_default(),
                )
            })
            .collect();

        Ok(results)
    }

    pub async fn collection_exists(
        &mut self,
        name: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let payloads = self
            .db
            .execute(&format!(
                "SELECT COUNT(*) AS count FROM collections WHERE name = {}",
                quote_sql_string(name)
            ))
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
        Ok(count > 0)
    }

    pub async fn remove_collection(
        &mut self,
        collection_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payloads = self
            .db
            .execute(&format!(
                "SELECT id FROM collections WHERE name = {}",
                quote_sql_string(collection_name)
            ))
            .await?;

        let collection_id = match payloads.into_iter().next() {
            Some(p) => p
                .rows_as::<IdRow>()?
                .into_iter()
                .next()
                .map(|row| row.id)
                .ok_or_else(|| format!("Collection '{}' not found", collection_name))?,
            None => return Err(format!("Collection '{}' not found", collection_name).into()),
        };

        self.db.execute("BEGIN").await?;

        let tx_result: Result<(), Box<dyn std::error::Error>> = async {
            let del_print_q = format!(
                "DELETE FROM printings WHERE collection_id = {}",
                collection_id
            );
            self.db.execute(&del_print_q).await?;

            let del_coll_q = format!("DELETE FROM collections WHERE id = {}", collection_id);
            self.db.execute(&del_coll_q).await?;

            Ok(())
        }
        .await;

        match tx_result {
            Ok(_) => {
                self.db.execute("COMMIT").await?;
            }
            Err(e) => {
                let _ = self.db.execute("ROLLBACK").await;
                return Err(e);
            }
        }

        let collection_dir = self.collections_dir.join(collection_name);
        if collection_dir.exists() {
            fs::remove_dir_all(&collection_dir)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_filename_variants() {
        assert_eq!(
            CollectionManager::parse_filename(Path::new("01001.jpg")),
            Some(("01001".to_string(), "original".to_string()))
        );

        assert_eq!(
            CollectionManager::parse_filename(Path::new("01001_alt1.jpg")),
            Some(("01001".to_string(), "alt1".to_string()))
        );

        assert_eq!(
            CollectionManager::parse_filename(Path::new("01001_Rear.png")),
            Some(("01001".to_string(), "rear".to_string()))
        );

        assert_eq!(
            CollectionManager::parse_filename(Path::new("notacode.jpg")),
            None
        );
        assert_eq!(
            CollectionManager::parse_filename(Path::new("abc_alt.jpg")),
            None
        );
    }
}
