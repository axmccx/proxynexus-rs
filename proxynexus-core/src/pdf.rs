use crate::card_source::CardSource;
use crate::card_store::CardStore;
use crate::db_storage::DbStorage;
use crate::image_provider::ImageProvider;
use image::ImageFormat;
use krilla::Data;
use krilla::Document;
use krilla::geom::{Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use std::collections::HashMap;
use tracing::info;
use web_time::Instant;

const POINTS_PER_INCH: f32 = 72.0;

const LETTER_WIDTH: f32 = 8.5 * POINTS_PER_INCH; // 612 points
const LETTER_HEIGHT: f32 = 11.0 * POINTS_PER_INCH; // 792 points
const A4_WIDTH: f32 = 8.27 * POINTS_PER_INCH; // ~595 points
const A4_HEIGHT: f32 = 11.69 * POINTS_PER_INCH; // ~842 points

const CARD_WIDTH: f32 = 178.54; // 6.299 cm in points
const CARD_HEIGHT: f32 = 249.09; // 8.788 cm in points

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum PageSize {
    #[default]
    Letter,
    A4,
}

impl PageSize {
    fn dimensions(&self) -> (f32, f32) {
        match self {
            PageSize::Letter => (LETTER_WIDTH, LETTER_HEIGHT),
            PageSize::A4 => (A4_WIDTH, A4_HEIGHT),
        }
    }

    fn margins(&self) -> (f32, f32) {
        match self {
            PageSize::Letter => (36.0, 21.0),
            PageSize::A4 => (30.0, 46.0),
        }
    }
}

fn calculate_card_position(card_index: usize, page_size: &PageSize) -> (f32, f32) {
    let (left_margin, top_margin) = page_size.margins();

    let col = card_index % 3;
    let row = card_index / 3;

    let x = left_margin + (col as f32 * CARD_WIDTH);
    let y = top_margin + (row as f32 * CARD_HEIGHT);

    (x, y)
}

pub async fn generate_pdf(
    db: &mut DbStorage,
    card_source: &impl CardSource,
    image_provider: &impl ImageProvider,
    page_size: PageSize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut store = CardStore::new(db)?;
    let card_requests = card_source.to_card_requests(&mut store).await?;

    let available = store.get_available_printings(&card_requests).await?;
    let printings = store.resolve_printings(&card_requests, &available)?;

    let mut image_keys: Vec<String> = Vec::new();
    for p in &printings {
        image_keys.push(p.image_key.clone());
        for part in &p.parts {
            image_keys.push(part.image_key.clone());
        }
    }

    let mut document = Document::new();
    let (page_width, page_height) = page_size.dimensions();

    let mut image_cache: HashMap<String, Image> = HashMap::new();

    for chunk in image_keys.chunks(9) {
        let page_settings = PageSettings::from_wh(page_width, page_height).unwrap();
        let mut page = document.start_page_with(page_settings);
        let mut surface = page.surface();

        for (index, image_key) in chunk.iter().enumerate() {
            let start = Instant::now();

            if !image_cache.contains_key(image_key) {
                let image_data = image_provider.get_image_bytes(image_key).await?;
                let format = image::guess_format(&image_data).unwrap_or(ImageFormat::Jpeg);

                let image = if format == ImageFormat::Png {
                    Image::from_png(Data::from(image_data), true)?
                } else {
                    Image::from_jpeg(Data::from(image_data), true)?
                };
                
                image_cache.insert(image_key.clone(), image);
            }

            let image = image_cache.get(image_key).unwrap().clone();
            let size = Size::from_wh(CARD_WIDTH, CARD_HEIGHT).unwrap();

            let (pos_x, pos_y) = calculate_card_position(index, &page_size);

            surface.push_transform(&Transform::from_translate(pos_x, pos_y));
            surface.draw_image(image, size);
            surface.pop();

            info!("Runtime for image {}: {:?}", image_key, start.elapsed());
        }

        surface.finish();
        page.finish();
    }

    let pdf = document.finish().unwrap();
    Ok(pdf)
}
