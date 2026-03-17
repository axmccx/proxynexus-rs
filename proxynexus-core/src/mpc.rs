use crate::border_generator::{apply_uniqueness_marker, create_bordered_base, encode_image};
use crate::card_source::CardSource;
use crate::card_store::CardStore;
use crate::db_storage::DbStorage;
use crate::image_provider::ImageProvider;
use crate::models::Printing;
use image::ImageFormat;
use std::collections::HashMap;
use std::io::{Cursor, Seek, Write};
use tracing::info;
use web_time::Instant;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

pub async fn generate_mpc_zip(
    db: &mut DbStorage,
    card_source: &impl CardSource,
    image_provider: &impl ImageProvider,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut store = CardStore::new(db)?;
    let card_requests = card_source.to_card_requests(&mut store).await?;

    let available = store.get_available_printings(&card_requests).await?;
    let printings = store.resolve_printings(&card_requests, &available)?;

    let mut sides: HashMap<String, Vec<Printing>> = HashMap::new();
    for printing in printings {
        sides
            .entry(printing.side.clone())
            .or_default()
            .push(printing);
    }

    let mut zip_buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut zip_buffer);

    let single_side = sides.len() == 1;
    let mut image_cache: HashMap<String, (image::RgbImage, ImageFormat)> = HashMap::new();

    for (side_name, side_printings) in sides {
        let folder_name = if single_side {
            "card-images".to_string()
        } else {
            format!("{}-images", side_name)
        };

        process_side(
            side_printings,
            image_provider,
            &mut zip,
            &folder_name,
            &mut image_cache,
        )
        .await?;
    }

    zip.finish()?;
    Ok(zip_buffer.into_inner())
}

async fn process_side<W: Write + Seek>(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    zip: &mut ZipWriter<W>,
    folder_name: &str,
    image_cache: &mut HashMap<String, (image::RgbImage, ImageFormat)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut copy_counters: HashMap<(String, String, String), u32> = HashMap::new();

    for printing in printings {
        let key = (
            printing.collection.clone(),
            printing.card_code.clone(),
            printing.variant.clone(),
        );
        let copy_num = copy_counters
            .entry(key)
            .and_modify(|n| *n += 1)
            .or_insert(1);

        let image_keys_to_process =
            std::iter::once(("front".to_string(), printing.image_key.clone()))
                .chain(printing.parts.into_iter().map(|a| (a.name, a.image_key)));

        for (part_name, current_image_key) in image_keys_to_process {
            let start = Instant::now();

            if !image_cache.contains_key(&current_image_key) {
                let data = image_provider.get_image_bytes(&current_image_key).await?;
                let image_format = image::guess_format(&data).unwrap_or(ImageFormat::Jpeg);
                let img = image::load_from_memory(&data)?;
                let bordered_base = create_bordered_base(&img);
                image_cache.insert(current_image_key.clone(), (bordered_base, image_format));
            }

            let (bordered_base, image_format) = image_cache.get(&current_image_key).unwrap();
            let mut final_image = bordered_base.clone();
            apply_uniqueness_marker(&mut final_image, *copy_num);
            let bordered_bytes = encode_image(final_image, *image_format)?;

            let ext = if *image_format == ImageFormat::Png {
                "png"
            } else {
                "jpg"
            };

            let filename = if part_name == "front" {
                format!(
                    "{}/{}-{}-{}-{}.{}",
                    folder_name,
                    printing.card_code,
                    printing.variant,
                    printing.collection,
                    copy_num,
                    ext
                )
            } else {
                format!(
                    "{}/{}-{}-{}-{}-{}.{}",
                    folder_name,
                    printing.card_code,
                    printing.variant,
                    printing.collection,
                    copy_num,
                    part_name,
                    ext
                )
            };

            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            zip.start_file(&filename, options)?;
            zip.write_all(&bordered_bytes)?;

            info!(
                "Runtime for image {}: {:?}",
                current_image_key,
                start.elapsed()
            );
        }
    }

    Ok(())
}
