use crate::ImageProvider;
use crate::border_generator::generate_bordered_image;
use crate::card_source::CardSource;
use crate::card_store::CardStore;
use crate::db_storage::DbStorage;
use crate::models::Printing;
use std::collections::HashMap;
use std::io::{Cursor, Seek, Write};
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

    for (side_name, side_printings) in sides {
        let folder_name = if single_side {
            "card-images".to_string()
        } else {
            format!("{}-images", side_name)
        };

        process_side(side_printings, image_provider, &mut zip, &folder_name).await?;
    }

    zip.finish()?;
    Ok(zip_buffer.into_inner())
}

async fn process_side<W: Write + Seek>(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    zip: &mut ZipWriter<W>,
    folder_name: &str,
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

        let image_data = image_provider.get_image_bytes(&printing.image_key).await?;
        let img = image::load_from_memory(&image_data)?;

        #[cfg(not(target_arch = "wasm32"))]
        let start = std::time::Instant::now();

        let bordered_bytes = generate_bordered_image(&img, *copy_num)?;

        #[cfg(not(target_arch = "wasm32"))]
        eprintln!(
            "generate_bordered_image runtime for {}: {:?}",
            printing.image_key,
            start.elapsed()
        );

        let filename = format!(
            "{}/{}-{}-{}-{}.jpg",
            folder_name, printing.card_code, printing.variant, printing.collection, copy_num
        );

        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip.start_file(&filename, options)?;
        zip.write_all(&bordered_bytes)?;
    }

    Ok(())
}
