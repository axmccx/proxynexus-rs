use chrono::Utc;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::{FileOptions, ZipWriter};

use crate::models::Manifest;

#[derive(Debug, Default)]
pub struct BuildReport {
    pub printings_added: usize,
    pub image_paths: Vec<PathBuf>,
}

pub fn build_collection(
    output_path: &Path,
    images_dir: &Path,
    language: String,
    version: String,
) -> Result<BuildReport, Box<dyn std::error::Error>> {
    let images = scan_images(images_dir);

    if images.is_empty() {
        return Err("No images found in images directory".into());
    }

    let manifest = Manifest {
        version,
        language,
        generated_date: Utc::now().to_rfc3339(),
    };

    let zip_file = File::create(output_path)?;
    let mut zip = ZipWriter::new(zip_file);
    let options: FileOptions<'_, ()> = FileOptions::default().unix_permissions(0o755);

    let manifest_toml = toml::to_string_pretty(&manifest)?;
    zip.start_file("manifest.toml", options)?;
    zip.write_all(manifest_toml.as_bytes())?;

    for path in &images {
        let filename = path
            .file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy();

        zip.start_file(format!("images/{}", filename), options)?;
        zip.write_all(&fs::read(path)?)?;
    }

    zip.finish()?;

    Ok(BuildReport {
        printings_added: images.len(),
        image_paths: images,
    })
}

fn scan_images(dir: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .filter(|p| {
            p.extension()
                .map(|ext| {
                    let ext = ext.to_string_lossy().to_lowercase();
                    ext == "jpg" || ext == "jpeg" || ext == "png"
                })
                .unwrap_or(false)
        })
        .collect()
}
