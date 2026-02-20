use crate::border_generator::generate_bordered_image;
use crate::card_query::CardQuery;
use crate::card_source::CardSource;
use crate::models::Printing;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

pub fn generate_mpc_zip(
    card_source: &impl CardSource,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let card_requests = card_source.to_card_requests()?;

    let query = CardQuery::new()?;
    let available = query.get_available_printings(&card_requests)?;
    let printings = query.resolve_printings(&card_requests, &available)?;

    let mut sides: HashMap<String, Vec<Printing>> = HashMap::new();

    for printing in printings {
        sides
            .entry(printing.side.clone())
            .or_insert_with(Vec::new)
            .push(printing);
    }

    let zip_file = File::create(output_path)?;
    let mut zip = ZipWriter::new(zip_file);

    let single_side = sides.len() == 1;

    for (side_name, side_printings) in sides {
        let folder_name = if single_side {
            "card-images".to_string()
        } else {
            format!("{}-images", side_name)
        };

        process_side(&mut zip, &folder_name, side_printings)?;
    }

    zip.finish()?;
    Ok(())
}

fn process_side(
    zip: &mut ZipWriter<File>,
    folder_name: &str,
    printings: Vec<Printing>,
) -> Result<(), Box<dyn std::error::Error>> {
    use opencv::imgcodecs;

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

        let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
        let input_path = home
            .join(".proxynexus")
            .join("collections")
            .join(&printing.file_path);

        if !input_path.exists() {
            return Err(format!("Image not found: {:?}", input_path).into());
        }
        let img = imgcodecs::imread(
            input_path.to_str().ok_or("Invalid input path encoding")?,
            imgcodecs::IMREAD_COLOR,
        )?;

        let bordered_bytes = generate_bordered_image(&img, *copy_num)?;

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
