mod collection;
pub mod collection_builder;
mod csv_parser;
mod db_schema;
mod image_scanner;

use krilla::Data;
use krilla::Document;
use krilla::geom::{Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use std::path::{Path, PathBuf};

const POINTS_PER_INCH: f32 = 72.0;

const LETTER_WIDTH: f32 = 8.5 * POINTS_PER_INCH; // 612 points
const LETTER_HEIGHT: f32 = 11.0 * POINTS_PER_INCH; // 792 points
const A4_WIDTH: f32 = 8.27 * POINTS_PER_INCH; // ~595 points
const A4_HEIGHT: f32 = 11.69 * POINTS_PER_INCH; // ~842 points

const CARD_WIDTH: f32 = 178.54; // 6.299 cm in points
const CARD_HEIGHT: f32 = 249.09; // 8.788 cm in points

pub enum PageSize {
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

pub fn create_pdf_with_images(
    image_paths: Vec<PathBuf>,
    output_path: &Path,
    page_size: PageSize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let (page_wight, page_width) = page_size.dimensions();

    for chuck in image_paths.chunks(9) {
        let page_settings = PageSettings::from_wh(page_wight, page_width).unwrap();
        let mut page = document.start_page_with(page_settings);
        let mut surface = page.surface();

        for (index, image_path) in chuck.iter().enumerate() {
            let image_data = std::fs::read(image_path)?;
            let image = Image::from_jpeg(Data::from(image_data), true)?;
            let size = Size::from_wh(CARD_WIDTH, CARD_HEIGHT).unwrap();

            let (pos_x, pos_y) = calculate_card_position(index, &page_size);

            surface.push_transform(&Transform::from_translate(pos_x, pos_y));
            surface.draw_image(image, size);
            surface.pop();
        }

        surface.finish();
        page.finish();
    }

    let pdf = document.finish().unwrap();
    std::fs::write(output_path, &pdf)?;

    Ok(())
}
