use crate::error::Result;
use crate::image_provider::ImageProvider;
use crate::models::Printing;
use crate::print_prep;
use async_trait::async_trait;
use image::ImageFormat;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Cursor, Seek, Write};
use tracing::info;
use web_time::Instant;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CardBackProvider {
    async fn fetch_card_backs(&self) -> Result<Vec<(String, Vec<u8>)>>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct MpcOptions {
    pub upscale: bool,
}

pub async fn generate_mpc_zip(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    options: MpcOptions,
    card_backs: Vec<(String, Vec<u8>)>,
    progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<u8>> {
    let total_images: usize = printings.iter().map(|p| 1 + p.parts.len()).sum();
    let mut processed_images = 0;

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

        process_side(
            side_printings,
            image_provider,
            options,
            &mut zip,
            &folder_name,
            &progress,
            &mut processed_images,
            total_images,
        )
        .await?;
    }

    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (filename, bytes) in card_backs {
        zip.start_file(filename, options)?;
        zip.write_all(&bytes)?;
    }

    zip.finish()?;
    Ok(zip_buffer.into_inner())
}

#[allow(clippy::too_many_arguments)]
async fn process_side<W: Write + Seek>(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    options: MpcOptions,
    zip: &mut ZipWriter<W>,
    folder_name: &str,
    progress: &Option<Box<dyn Fn(f32) + Send + Sync>>,
    processed_images: &mut usize,
    total_images: usize,
) -> Result<()> {
    let mut copy_counters: HashMap<(String, String, Option<String>), u32> = HashMap::new();
    let mut uniqueness_counter: u32 = 0;

    struct ImageRequest {
        printing: Printing,
        part_name: String,
        image_key: String,
        copy_num: u32,
        has_bleed: bool,
    }

    let mut requests: Vec<ImageRequest> = Vec::new();

    for printing in printings {
        let key = (
            printing.collection.clone(),
            printing.card_id.clone(),
            printing.variant.clone(),
        );
        let copy_num = copy_counters
            .entry(key)
            .and_modify(|n| *n += 1)
            .or_insert(1);

        let (front_key, front_has_bleed) = printing.mpc_image();
        let parts = printing.parts.clone();

        requests.push(ImageRequest {
            printing: printing.clone(),
            part_name: "front".to_string(),
            image_key: front_key,
            copy_num: *copy_num,
            has_bleed: front_has_bleed,
        });

        for part in parts {
            let (part_key, part_has_bleed) = part.mpc_image();
            requests.push(ImageRequest {
                printing: printing.clone(),
                part_name: part.name,
                image_key: part_key,
                copy_num: *copy_num,
                has_bleed: part_has_bleed,
            });
        }
    }

    requests.sort_by(|a, b| a.image_key.cmp(&b.image_key));

    struct CachedImage {
        key: String,
        image: image::RgbImage,
        format: ImageFormat,
    }

    let mut current_cache: Option<CachedImage> = None;

    for req in requests {
        let printing = req.printing;
        let part_name = req.part_name;
        let current_image_key = req.image_key;
        let copy_num = req.copy_num;

        uniqueness_counter += 1;
        let start = Instant::now();

        if current_cache
            .as_ref()
            .is_none_or(|c| c.key != current_image_key)
        {
            let mut image_data = image_provider.get_image_bytes(&current_image_key).await?;

            if options.upscale {
                image_data = crate::upscale_image(&image_data).await?
            }

            let image_format = image::guess_format(&image_data).unwrap_or(ImageFormat::Jpeg);
            let img = image::load_from_memory(&image_data)?;
            let bleed_image = if req.has_bleed {
                img.to_rgb8()
            } else {
                print_prep::add_mpc_bleed_border(&img)
            };

            current_cache = Some(CachedImage {
                key: current_image_key.clone(),
                image: bleed_image,
                format: image_format,
            });
        }

        let cached = current_cache.as_ref().unwrap();
        let mut final_image = cached.image.clone();
        print_prep::apply_uniqueness_marker(&mut final_image, uniqueness_counter);
        let bordered_bytes = print_prep::encode_image(final_image, cached.format)?;

        let ext = if cached.format == ImageFormat::Png {
            "png"
        } else {
            "jpg"
        };

        let variant_label = printing.variant.as_deref().unwrap_or("official");

        let filename = if part_name == "front" {
            format!(
                "{}/{}-{}-{}-{}.{}",
                folder_name, printing.card_id, variant_label, printing.collection, copy_num, ext
            )
        } else {
            format!(
                "{}/{}-{}-{}-{}-{}.{}",
                folder_name,
                printing.card_id,
                variant_label,
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

        *processed_images += 1;
        if let Some(cb) = progress
            && total_images > 0
        {
            cb(*processed_images as f32 / total_images as f32);
        }

        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(0).await;

        info!(
            "Runtime for image {}: {:?}",
            current_image_key,
            start.elapsed()
        );
    }

    Ok(())
}
