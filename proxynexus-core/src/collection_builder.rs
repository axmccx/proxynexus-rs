use chrono::Utc;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use zip::write::{FileOptions, ZipWriter};

use crate::db::collection_schema;
use crate::models::{CardMetadata, CollectionPrinting, Manifest};
use crate::{csv_parser, image_scanner};

#[derive(Debug, Default)]
pub struct BuildReport {
    pub cards_added: usize,
    pub printings_added: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub struct CollectionBuilder {
    output_path: PathBuf,
    images_dir: PathBuf,
    metadata_csv: PathBuf,
    version: String,
    language: String,
    verbose: bool,
}

impl CollectionBuilder {
    pub fn new(
        output_path: PathBuf,
        images_dir: PathBuf,
        metadata_csv: PathBuf,
        language: String,
        version: String,
    ) -> Self {
        Self {
            output_path,
            images_dir,
            metadata_csv,
            version,
            language,
            verbose: false,
        }
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn build(self) -> Result<BuildReport, Box<dyn std::error::Error>> {
        let mut report = BuildReport::default();

        let cards = csv_parser::parse_csv(&self.metadata_csv)?;
        let images = image_scanner::scan_images(&self.images_dir);

        let card_map: HashMap<String, &CardMetadata> =
            cards.iter().map(|c| (c.code.clone(), c)).collect();

        let mut image_map: HashMap<String, Vec<image_scanner::ScannedImage>> = HashMap::new();
        for img in images {
            image_map.entry(img.code.clone()).or_default().push(img);
        }

        for code in image_map.keys() {
            if !card_map.contains_key(code) {
                report.errors.push(format!(
                    "Image file(s) for code '{}' found but no corresponding card in CSV",
                    code
                ));
            }
        }

        if !report.errors.is_empty() {
            eprintln!("\nBuild failed with {} error(s):\n", report.errors.len());
            for err in &report.errors {
                eprintln!("   ERROR: {}", err);
            }
            return Err("Build failed: orphaned image files detected".into());
        }

        let temp_dir = std::env::temp_dir().join(format!(
            "proxynexus_build_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
        ));
        fs::create_dir_all(&temp_dir)?;
        let db_path = temp_dir.join("index.db");
        let conn = Connection::open(&db_path)?;
        collection_schema::create_collection_schema(&conn)?;

        for card in &cards {
            if let Some(images) = image_map.get(&card.code) {
                collection_schema::insert_card(&conn, card)?;
                report.cards_added += 1;

                for img in images {
                    let filename = img
                        .path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .ok_or("Invalid image filename")?;

                    let printing = CollectionPrinting {
                        card_code: card.code.clone(),
                        variant: img.variant.clone(),
                        file_path: filename.to_string(),
                    };

                    collection_schema::insert_printing(&conn, &printing)?;
                    report.printings_added += 1;
                }

                let variant_text = match images.len() {
                    1 => "1 printing".to_string(),
                    n => format!("{} printings", n),
                };
                println!("Found: {} {} ({})", card.code, card.title, variant_text);
            } else {
                let warning = format!(
                    "Card '{}' ({}) found in CSV but has no images - skipping",
                    card.code, card.title
                );
                report.warnings.push(warning.clone());
                if self.verbose {
                    println!("{} - {} (no images, skipped)", card.code, card.title);
                }
            }
        }

        drop(conn);

        let manifest = Manifest {
            version: self.version,
            language: self.language,
            generated_date: Utc::now().to_rfc3339(),
        };

        let manifest_path = temp_dir.join("manifest.toml");
        let manifest_toml = toml::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_toml)?;

        println!("Writing pnx file...");
        let zip_file = File::create(&self.output_path)?;
        let mut zip = ZipWriter::new(zip_file);

        let options: FileOptions<'_, ()> = FileOptions::default().unix_permissions(0o755);

        zip.start_file("manifest.toml", options)?;
        let manifest_content = fs::read(&manifest_path)?;
        zip.write_all(&manifest_content)?;

        zip.start_file("index.db", options)?;
        let db_content = fs::read(&db_path)?;
        zip.write_all(&db_content)?;

        for images in image_map.values() {
            for img in images {
                let filename = img
                    .path
                    .file_name()
                    .ok_or("Invalid filename")?
                    .to_string_lossy();
                let zip_path = format!("images/{}", filename);

                zip.start_file(&zip_path, options)?;
                let image_content = fs::read(&img.path)?;
                zip.write_all(&image_content)?;
            }
        }

        zip.finish()?;

        fs::remove_dir_all(&temp_dir)?;

        println!("Cards added: {}", report.cards_added);
        println!("Printings added: {}", report.printings_added);
        println!("Collection created: {}", self.output_path.display());

        if !report.warnings.is_empty() && self.verbose {
            println!("\nWarnings ({}):", report.warnings.len());
            for warn in &report.warnings {
                println!("  {}", warn);
            }
        }

        Ok(report)
    }
}
