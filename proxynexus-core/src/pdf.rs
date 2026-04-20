use crate::error::{ProxyNexusError, Result};
use crate::image_provider::ImageProvider;
use crate::models::Printing;
use image::ImageFormat;
use krilla::Data;
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::Stroke;
use serde::Serialize;
use std::collections::HashMap;
use tracing::info;
use web_time::Instant;

const POINTS_PER_INCH: f32 = 72.0;
const MM_TO_POINTS: f32 = POINTS_PER_INCH / 25.4;

const LETTER_WIDTH: f32 = 8.5 * POINTS_PER_INCH; // 612 points
const LETTER_HEIGHT: f32 = 11.0 * POINTS_PER_INCH; // 792 points
const A4_WIDTH: f32 = 8.27 * POINTS_PER_INCH; // ~595 points
const A4_HEIGHT: f32 = 11.69 * POINTS_PER_INCH; // ~842 points

const CARD_WIDTH: f32 = 178.54; // 6.299 cm in points
const CARD_HEIGHT: f32 = 249.09; // 8.788 cm in points

const MINIMUM_MARGIN: f32 = 0.25 * POINTS_PER_INCH;

pub const MIN_CUT_LINE_THICKNESS: f32 = 0.1;
pub const MAX_CUT_LINE_THICKNESS: f32 = 10.0;
pub const DEFAULT_CUT_LINE_THICKNESS: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize)]
pub enum PageSize {
    #[default]
    Letter,
    A4,
    Custom(f32, f32),
}

impl PageSize {
    fn dimensions(&self) -> (f32, f32) {
        match self {
            PageSize::Letter => (LETTER_WIDTH, LETTER_HEIGHT),
            PageSize::A4 => (A4_WIDTH, A4_HEIGHT),
            PageSize::Custom(width, height) => (width * POINTS_PER_INCH, height * POINTS_PER_INCH),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub enum CutLines {
    None,
    #[default]
    Margins,
    FullPage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub enum PrintLayout {
    #[default]
    EdgeToEdge,
    Gap,
    SmallMargin,
    LargeMargin,
}

impl PrintLayout {
    fn gap_points(&self) -> f32 {
        match self {
            PrintLayout::Gap => 0.125 * POINTS_PER_INCH,
            _ => 0.0,
        }
    }

    fn inset_points(&self) -> f32 {
        match self {
            PrintLayout::SmallMargin => 1.0 * MM_TO_POINTS,
            PrintLayout::LargeMargin => 2.0 * MM_TO_POINTS,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PdfOptions {
    pub page_size: PageSize,
    pub cut_lines: CutLines,
    pub print_layout: PrintLayout,
    pub cut_line_thickness: f32,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            page_size: PageSize::default(),
            cut_lines: CutLines::default(),
            print_layout: PrintLayout::default(),
            cut_line_thickness: DEFAULT_CUT_LINE_THICKNESS,
        }
    }
}

impl PdfOptions {
    fn effective_gap(&self) -> f32 {
        let base = self.print_layout.gap_points();
        match self.cut_lines {
            CutLines::FullPage => base.max(self.cut_line_thickness),
            _ => base,
        }
    }

    fn capacity(&self) -> (usize, usize) {
        let (page_width, page_height) = self.page_size.dimensions();
        let gap = self.effective_gap();
        let max_cols =
            ((page_width - (MINIMUM_MARGIN * 2.0) + gap) / (CARD_WIDTH + gap)).floor() as usize;
        let max_rows =
            ((page_height - (MINIMUM_MARGIN * 2.0) + gap) / (CARD_HEIGHT + gap)).floor() as usize;
        (max_rows, max_cols)
    }

    fn margins(&self) -> (f32, f32) {
        let (page_width, page_height) = self.page_size.dimensions();
        let (max_rows, max_cols) = self.capacity();
        let gap = self.effective_gap();

        let left_margin =
            (page_width - (max_cols as f32 * CARD_WIDTH + (max_cols as f32 - 1.0) * gap)) / 2.0;
        let top_margin =
            (page_height - (max_rows as f32 * CARD_HEIGHT + (max_rows as f32 - 1.0) * gap)) / 2.0;

        (left_margin, top_margin)
    }
}

pub async fn generate_pdf(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    options: PdfOptions,
    progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<u8>> {
    let total_images: usize = printings.iter().map(|p| 1 + p.parts.len()).sum();
    let mut processed_images: usize = 0;

    let mut image_keys: Vec<String> = Vec::with_capacity(total_images);
    for p in &printings {
        image_keys.push(p.image_key.clone());
        for part in &p.parts {
            image_keys.push(part.image_key.clone());
        }
    }

    let mut image_cache: HashMap<String, Image> = HashMap::new();
    let mut document = Document::new();
    let (page_width, page_height) = options.page_size.dimensions();

    let (max_rows, max_cols) = options.capacity();
    let max_cards_per_page = max_rows * max_cols;

    for chunk in image_keys.chunks(max_cards_per_page) {
        let page_settings = PageSettings::from_wh(page_width, page_height).unwrap();
        let mut page = document.start_page_with(page_settings);
        let mut surface = page.surface();

        for (index, image_key) in chunk.iter().enumerate() {
            let start = Instant::now();

            if !image_cache.contains_key(image_key) {
                let image_data = image_provider.get_image_bytes(image_key).await?;
                let format = image::guess_format(&image_data).unwrap_or(ImageFormat::Jpeg);

                let image = if format == ImageFormat::Png {
                    Image::from_png(Data::from(image_data), true)
                        .map_err(|e| ProxyNexusError::Internal(e.to_string()))?
                } else {
                    Image::from_jpeg(Data::from(image_data), true)
                        .map_err(|e| ProxyNexusError::Internal(e.to_string()))?
                };

                image_cache.insert(image_key.clone(), image);
            }

            let image = image_cache.get(image_key).unwrap().clone();
            let (pos_x, pos_y) = calculate_card_position(index, &options);
            let inset = options.print_layout.inset_points();

            let draw_x = pos_x + inset;
            let draw_y = pos_y + inset;
            let draw_width = CARD_WIDTH - (2.0 * inset);
            let draw_height = CARD_HEIGHT - (2.0 * inset);

            let size = Size::from_wh(draw_width, draw_height).unwrap();

            surface.push_transform(&Transform::from_translate(draw_x, draw_y));
            surface.draw_image(image, size);
            surface.pop();

            processed_images += 1;
            if let Some(ref cb) = progress
                && total_images > 0
            {
                cb(processed_images as f32 / total_images as f32);
            }

            #[cfg(not(target_arch = "wasm32"))]
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(0).await;

            info!("Runtime for image {}: {:?}", image_key, start.elapsed());
        }

        surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(16, 16, 16).into(),
            width: options.cut_line_thickness,
            miter_limit: 0.0,
            line_cap: Default::default(),
            line_join: Default::default(),
            opacity: NormalizedF32::new(1.0).unwrap(),
            dash: None,
        }));

        let lines = match options.cut_lines {
            CutLines::None => Vec::new(),
            CutLines::Margins => calculate_margin_cutlines(&options),
            CutLines::FullPage => calculate_full_page_cutlines(&options),
        };

        for line in &lines {
            surface.draw_path(line);
        }

        surface.finish();
        page.finish();
    }

    let pdf = document.finish().unwrap();
    Ok(pdf)
}

fn calculate_card_position(card_index: usize, options: &PdfOptions) -> (f32, f32) {
    let (_, max_cols) = options.capacity();
    let (left_margin, top_margin) = options.margins();
    let gap = options.effective_gap();

    let col = (card_index % max_cols) as f32;
    let row = (card_index / max_cols) as f32;

    let x = left_margin + (col * CARD_WIDTH) + (col * gap);
    let y = top_margin + (row * CARD_HEIGHT) + (row * gap);

    (x, y)
}

fn calculate_margin_cutlines(options: &PdfOptions) -> Vec<Path> {
    let (max_rows, max_cols) = options.capacity();
    let (left_margin, top_margin) = options.margins();
    let gap = options.effective_gap();
    let line_length: f32 = 15.0;
    let line_gap: f32 = 3.0_f32.max(options.cut_line_thickness / 2.0 + 1.0);

    let mut lines = Vec::<Path>::new();

    let right_x = left_margin + (max_cols as f32 * CARD_WIDTH + (max_cols as f32 - 1.0) * gap);
    let bottom_y = top_margin + (max_rows as f32 * CARD_HEIGHT + (max_rows as f32 - 1.0) * gap);

    // top cut lines
    for i in 0..=max_cols {
        let x = if i == 0 {
            left_margin
        } else {
            left_margin + i as f32 * CARD_WIDTH + (i as f32 - 1.0) * gap
        };

        let mut pb = PathBuilder::new();
        pb.move_to(x, top_margin - line_length - line_gap);
        pb.line_to(x, top_margin - line_gap);
        lines.push(pb.finish().unwrap());

        if gap > 0.0 && i > 0 && i < max_cols {
            let x_gap = x + gap;
            let mut pb = PathBuilder::new();
            pb.move_to(x_gap, top_margin - line_length - line_gap);
            pb.line_to(x_gap, top_margin - line_gap);
            lines.push(pb.finish().unwrap());
        }
    }

    // bottom cut lines
    for i in 0..=max_cols {
        let x = if i == 0 {
            left_margin
        } else {
            left_margin + i as f32 * CARD_WIDTH + (i as f32 - 1.0) * gap
        };

        let mut pb = PathBuilder::new();
        pb.move_to(x, bottom_y + line_gap);
        pb.line_to(x, bottom_y + line_length + line_gap);
        lines.push(pb.finish().unwrap());

        if gap > 0.0 && i > 0 && i < max_cols {
            let x_gap = x + gap;
            let mut pb = PathBuilder::new();
            pb.move_to(x_gap, bottom_y + line_gap);
            pb.line_to(x_gap, bottom_y + line_length + line_gap);
            lines.push(pb.finish().unwrap());
        }
    }

    // left cut lines
    for i in 0..=max_rows {
        let y = if i == 0 {
            top_margin
        } else {
            top_margin + i as f32 * CARD_HEIGHT + (i as f32 - 1.0) * gap
        };

        let mut pb = PathBuilder::new();
        pb.move_to(left_margin - line_length - line_gap, y);
        pb.line_to(left_margin - line_gap, y);
        lines.push(pb.finish().unwrap());

        if gap > 0.0 && i > 0 && i < max_rows {
            let y_gap = y + gap;
            let mut pb = PathBuilder::new();
            pb.move_to(left_margin - line_length - line_gap, y_gap);
            pb.line_to(left_margin - line_gap, y_gap);
            lines.push(pb.finish().unwrap());
        }
    }

    // right cut lines
    for i in 0..=max_rows {
        let y = if i == 0 {
            top_margin
        } else {
            top_margin + i as f32 * CARD_HEIGHT + (i as f32 - 1.0) * gap
        };

        let mut pb = PathBuilder::new();
        pb.move_to(right_x + line_gap, y);
        pb.line_to(right_x + line_length + line_gap, y);
        lines.push(pb.finish().unwrap());

        if gap > 0.0 && i > 0 && i < max_rows {
            let y_gap = y + gap;
            let mut pb = PathBuilder::new();
            pb.move_to(right_x + line_gap, y_gap);
            pb.line_to(right_x + line_length + line_gap, y_gap);
            lines.push(pb.finish().unwrap());
        }
    }

    lines
}

fn calculate_full_page_cutlines(options: &PdfOptions) -> Vec<Path> {
    let (max_rows, max_cols) = options.capacity();
    let (left_margin, top_margin) = options.margins();
    let (page_width, page_height) = options.page_size.dimensions();
    let gap = options.effective_gap();

    let mut lines = Vec::<Path>::new();

    for i in 0..=max_cols {
        let x = left_margin + (i as f32 * CARD_WIDTH) + ((i as f32 - 0.5) * gap);

        let mut pb = PathBuilder::new();
        pb.move_to(x, 0.0);
        pb.line_to(x, page_height);
        lines.push(pb.finish().unwrap());
    }

    for i in 0..=max_rows {
        let y = top_margin + (i as f32 * CARD_HEIGHT) + ((i as f32 - 0.5) * gap);

        let mut pb = PathBuilder::new();
        pb.move_to(0.0, y);
        pb.line_to(page_width, y);
        lines.push(pb.finish().unwrap());
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(cut_lines: CutLines, print_layout: PrintLayout, thickness: f32) -> PdfOptions {
        PdfOptions {
            page_size: PageSize::Letter,
            cut_lines,
            print_layout,
            cut_line_thickness: thickness,
        }
    }

    #[test]
    fn default_thickness_uses_the_default_constant() {
        assert_eq!(
            PdfOptions::default().cut_line_thickness,
            DEFAULT_CUT_LINE_THICKNESS
        );
    }

    #[test]
    fn thickness_constants_are_ordered_and_positive() {
        assert!(MIN_CUT_LINE_THICKNESS > 0.0);
        assert!(MIN_CUT_LINE_THICKNESS < DEFAULT_CUT_LINE_THICKNESS);
        assert!(DEFAULT_CUT_LINE_THICKNESS < MAX_CUT_LINE_THICKNESS);
    }

    #[test]
    fn effective_gap_ignores_thickness_for_none_and_margins() {
        let base = PrintLayout::EdgeToEdge.gap_points();
        assert_eq!(
            opts(
                CutLines::None,
                PrintLayout::EdgeToEdge,
                MAX_CUT_LINE_THICKNESS
            )
            .effective_gap(),
            base,
        );
        assert_eq!(
            opts(
                CutLines::Margins,
                PrintLayout::EdgeToEdge,
                MAX_CUT_LINE_THICKNESS
            )
            .effective_gap(),
            base,
        );
    }

    #[test]
    fn effective_gap_widens_for_full_page_when_thickness_exceeds_base() {
        let o = opts(CutLines::FullPage, PrintLayout::EdgeToEdge, 3.0);
        assert_eq!(o.effective_gap(), 3.0);
    }

    #[test]
    fn effective_gap_preserves_base_for_full_page_when_base_exceeds_thickness() {
        // Gap layout reserves 0.125in (9pt) between cards; a 0.5pt stroke shouldn't shrink it.
        let base = PrintLayout::Gap.gap_points();
        assert!(base > DEFAULT_CUT_LINE_THICKNESS);
        assert_eq!(
            opts(
                CutLines::FullPage,
                PrintLayout::Gap,
                DEFAULT_CUT_LINE_THICKNESS
            )
            .effective_gap(),
            base,
        );
    }

    #[test]
    fn full_page_thick_lines_can_reduce_capacity() {
        let thin = opts(
            CutLines::FullPage,
            PrintLayout::EdgeToEdge,
            DEFAULT_CUT_LINE_THICKNESS,
        );
        let thick = opts(
            CutLines::FullPage,
            PrintLayout::EdgeToEdge,
            MAX_CUT_LINE_THICKNESS,
        );
        let (thin_rows, thin_cols) = thin.capacity();
        let (thick_rows, thick_cols) = thick.capacity();
        assert!(thick_rows <= thin_rows);
        assert!(thick_cols <= thin_cols);
        assert!(
            thick_rows < thin_rows || thick_cols < thin_cols,
            "max thickness should cost at least one row or column vs default thickness"
        );
    }

    #[test]
    fn margin_capacity_is_independent_of_thickness() {
        let thin = opts(
            CutLines::Margins,
            PrintLayout::EdgeToEdge,
            MIN_CUT_LINE_THICKNESS,
        );
        let thick = opts(
            CutLines::Margins,
            PrintLayout::EdgeToEdge,
            MAX_CUT_LINE_THICKNESS,
        );
        assert_eq!(thin.capacity(), thick.capacity());
    }
}
