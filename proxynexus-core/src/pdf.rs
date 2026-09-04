use crate::error::{ProxyNexusError, Result};
use crate::image_provider::ImageProvider;
use crate::models::{
    BleedPreference, CardSide, PrintedCard, Printing, SourceImage, expand_to_cards,
};
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
use std::collections::{HashMap, HashSet};
use tracing::{info, warn};
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

const LAYOUT_GAP: f32 = 0.125 * POINTS_PER_INCH; // Gap layout, 9 points
const LAYOUT_INSET: f32 = 1.0 * MM_TO_POINTS; // Margin layout, ~2.83 points

pub const MIN_CUT_LINE_THICKNESS: f32 = 0.1;
pub const MAX_CUT_LINE_THICKNESS: f32 = 10.0;
pub const DEFAULT_CUT_LINE_THICKNESS: f32 = 0.5;

pub const PDF_BLEED_MM: f32 = 1.0;

const CARD_BACK_CACHE_PREFIX: &str = "card-back:";

type Page = Vec<(usize, Slot)>;

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
    Margin,
    Bleed,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PdfOptions {
    pub page_size: PageSize,
    pub cut_lines: CutLines,
    pub print_layout: PrintLayout,
    pub cut_line_thickness: f32,
    pub upscale: bool,
    pub double_sided: bool,
    pub back_label: Option<&'static str>,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            page_size: PageSize::default(),
            cut_lines: CutLines::default(),
            print_layout: PrintLayout::default(),
            cut_line_thickness: DEFAULT_CUT_LINE_THICKNESS,
            upscale: false,
            double_sided: false,
            back_label: None,
        }
    }
}

impl PdfOptions {
    fn effective_gap(&self) -> f32 {
        let base = match self.print_layout {
            PrintLayout::Gap => LAYOUT_GAP,
            PrintLayout::Bleed => 2.0 * self.bleed_pt(),
            PrintLayout::EdgeToEdge | PrintLayout::Margin => 0.0,
        };
        match self.cut_lines {
            CutLines::FullPage => base.max(self.cut_line_thickness),
            _ => base,
        }
    }

    fn base_gap(&self) -> f32 {
        match self.cut_lines {
            CutLines::FullPage => self.cut_line_thickness,
            _ => 0.0,
        }
    }

    fn base_capacity(&self) -> (usize, usize) {
        let (page_width, page_height) = self.page_size.dimensions();
        let gap = self.base_gap();
        let max_cols = ((page_width - (MINIMUM_MARGIN * 2.0) + gap) / (CARD_WIDTH + gap))
            .floor()
            .max(0.0) as usize;
        let max_rows = ((page_height - (MINIMUM_MARGIN * 2.0) + gap) / (CARD_HEIGHT + gap))
            .floor()
            .max(0.0) as usize;
        (max_rows, max_cols)
    }

    fn bleed_pt(&self) -> f32 {
        if self.print_layout != PrintLayout::Bleed {
            return 0.0;
        }

        let (rows, cols) = self.base_capacity();
        if rows == 0 || cols == 0 {
            return 0.0;
        }

        let (page_width, page_height) = self.page_size.dimensions();
        let gap = self.base_gap();

        let free_w = page_width
            - (MINIMUM_MARGIN * 2.0)
            - ((cols - 1) as f32 * gap)
            - (cols as f32 * CARD_WIDTH);
        let free_h = page_height
            - (MINIMUM_MARGIN * 2.0)
            - ((rows - 1) as f32 * gap)
            - (rows as f32 * CARD_HEIGHT);

        let fits_w = free_w / (2.0 * cols as f32);
        let fits_h = free_h / (2.0 * rows as f32);

        (PDF_BLEED_MM * MM_TO_POINTS)
            .min(fits_w)
            .min(fits_h)
            .max(0.0)
    }

    pub fn bleed_mm(&self) -> f32 {
        self.bleed_pt() / MM_TO_POINTS
    }

    fn bleed_ratio(&self) -> f32 {
        self.bleed_pt() / CARD_WIDTH
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

#[derive(Clone, Debug, PartialEq)]
enum Slot {
    Collection(SourceImage),
    CardBack(&'static str, bool),
    Blank,
}

impl Slot {
    fn from_side(side: &CardSide, preferred: BleedPreference) -> Self {
        match side.image(preferred) {
            Some(source) => Slot::Collection(source),
            None => Slot::Blank,
        }
    }

    fn cache_key(&self) -> Option<String> {
        match self {
            Slot::Collection(source) => Some(source.key.clone()),
            Slot::CardBack(asset_id, _) => Some(format!("{}{}", CARD_BACK_CACHE_PREFIX, asset_id)),
            Slot::Blank => None,
        }
    }

    fn has_bleed(&self) -> bool {
        match self {
            Slot::Collection(source) => source.has_bleed,
            Slot::CardBack(_, has_bleed) => *has_bleed,
            Slot::Blank => false,
        }
    }

    fn upscalable(&self) -> bool {
        matches!(self, Slot::Collection(..))
    }

    async fn load(&self, image_provider: &impl ImageProvider) -> Result<Vec<u8>> {
        match self {
            Slot::Collection(source) => image_provider.get_image_bytes(&source.key).await,
            Slot::CardBack(asset_id, _) => crate::card_backs::load_card_back(asset_id).await,
            Slot::Blank => Ok(Vec::new()),
        }
    }
}

pub async fn generate_pdf(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    game_id: &str,
    options: PdfOptions,
    progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<u8>> {
    let bleed_ratio = options.bleed_ratio();

    let preferred = if bleed_ratio > 0.0 {
        BleedPreference::Bleed
    } else {
        BleedPreference::NoBleed
    };

    let mut image_cache: HashMap<String, Image> = HashMap::new();
    let mut document = Document::new();
    let (page_width, page_height) = options.page_size.dimensions();

    let (max_rows, max_cols) = options.capacity();
    let max_cards_per_page = max_rows * max_cols;

    if bleed_ratio > 0.0 {
        info!(
            "bleed layout: {}x{} grid, {:.2}mm per side (target {:.2}mm)",
            max_rows,
            max_cols,
            options.bleed_mm(),
            PDF_BLEED_MM
        );
    }

    let cards: Vec<PrintedCard> = expand_to_cards(&printings)
        .into_iter()
        .map(|(_, card)| card)
        .collect();

    let pages = build_pages(
        &cards,
        &options,
        game_id,
        preferred,
        max_cards_per_page,
        max_cols,
    );

    let total_images: usize = pages
        .iter()
        .flatten()
        .filter(|(_, slot)| slot.cache_key().is_some())
        .count();
    let mut processed_images: usize = 0;

    for page_slots in &pages {
        let page_settings = PageSettings::from_wh(page_width, page_height).unwrap();
        let mut page = document.start_page_with(page_settings);
        let mut surface = page.surface();

        for (index, slot) in page_slots {
            let start = Instant::now();

            let Some(image_key) = slot.cache_key() else {
                continue;
            };

            if !image_cache.contains_key(&image_key) {
                let raw = slot.load(image_provider).await?;
                let format = image::guess_format(&raw).unwrap_or(ImageFormat::Jpeg);

                let upscaled = if options.upscale && slot.upscalable() {
                    let max = crate::print_prep::max_upscale_size(slot.has_bleed());
                    Some(crate::upscale_image(&raw, max).await?)
                } else {
                    None
                };

                let image_data = if slot.has_bleed() || bleed_ratio > 0.0 {
                    let img = match upscaled {
                        Some(rgb) => image::DynamicImage::ImageRgb8(rgb),
                        None => image::load_from_memory(&raw)?,
                    };
                    let prepared = if slot.has_bleed() {
                        crate::print_prep::crop_bleed_border(&img, bleed_ratio).to_rgb8()
                    } else {
                        crate::print_prep::add_uniform_bleed_border(&img, bleed_ratio)
                    };
                    crate::print_prep::encode_image(prepared, format)?
                } else {
                    match upscaled {
                        Some(rgb) => crate::print_prep::encode_image(rgb, format)?,
                        None => raw,
                    }
                };

                let image = if format == ImageFormat::Png {
                    Image::from_png(Data::from(image_data), true)
                        .map_err(|e| ProxyNexusError::Internal(e.to_string()))?
                } else {
                    Image::from_jpeg(Data::from(image_data), true)
                        .map_err(|e| ProxyNexusError::Internal(e.to_string()))?
                };

                image_cache.insert(image_key.clone(), image);
            } else {
                info!("cache hit for {}", image_key);
            }

            let image = image_cache.get(&image_key).unwrap().clone();
            let (draw_x, draw_y, draw_width, draw_height) = calculate_draw_rect(*index, &options);

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

fn build_pages(
    cards: &[PrintedCard],
    options: &PdfOptions,
    game_id: &str,
    preferred: BleedPreference,
    per_page: usize,
    max_cols: usize,
) -> Vec<Page> {
    if per_page == 0 {
        return Vec::new();
    }

    if !options.double_sided {
        let mut slots = Vec::new();
        for card in cards {
            slots.push(Slot::from_side(card.front, preferred));
            if let Some(back) = card.back {
                slots.push(Slot::from_side(back, preferred));
            }
        }

        return slots
            .chunks(per_page)
            .map(|chunk| chunk.iter().cloned().enumerate().collect())
            .collect();
    }

    let mut unbacked_groups = HashSet::new();
    let mut pages = Vec::new();
    let (mut own_backs, mut standard_backs, mut blanks) = (0usize, 0usize, 0usize);

    for chunk in cards.chunks(per_page) {
        pages.push(
            chunk
                .iter()
                .enumerate()
                .map(|(index, card)| (index, Slot::from_side(card.front, preferred)))
                .collect(),
        );
        pages.push(
            chunk
                .iter()
                .enumerate()
                .map(|(index, card)| {
                    let slot = back_slot(
                        card,
                        game_id,
                        options.back_label,
                        preferred,
                        &mut unbacked_groups,
                    );
                    match slot {
                        Slot::Collection(..) => own_backs += 1,
                        Slot::CardBack(..) => standard_backs += 1,
                        Slot::Blank => blanks += 1,
                    }
                    (mirror(index, max_cols), slot)
                })
                .collect(),
        );
    }

    info!(
        "double-sided: {} cards print their own back, {} the game's standard back, {} blank",
        own_backs, standard_backs, blanks
    );

    pages
}

fn back_slot(
    card: &PrintedCard,
    game_id: &str,
    label: Option<&'static str>,
    preferred: BleedPreference,
    unbacked_groups: &mut HashSet<String>,
) -> Slot {
    if let Some(back) = card.back {
        return Slot::from_side(back, preferred);
    }

    let group = &card.printing.back_group;
    if let Some(back) = label.and_then(|label| crate::card_backs::card_back(game_id, group, label))
    {
        return Slot::CardBack(back.asset_id, back.has_bleed);
    }

    if unbacked_groups.insert(group.clone()) {
        warn!(
            "No card back for the '{}' back group, so those cards print with a blank reverse.",
            group
        );
    }
    Slot::Blank
}

fn mirror(index: usize, max_cols: usize) -> usize {
    let row = index / max_cols;
    let col = index % max_cols;
    row * max_cols + (max_cols - 1 - col)
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

fn calculate_draw_rect(card_index: usize, options: &PdfOptions) -> (f32, f32, f32, f32) {
    let (pos_x, pos_y) = calculate_card_position(card_index, options);
    let inset = match options.print_layout {
        PrintLayout::Margin => LAYOUT_INSET,
        PrintLayout::EdgeToEdge | PrintLayout::Gap | PrintLayout::Bleed => 0.0,
    };
    let bleed = options.bleed_pt();

    (
        pos_x + inset - bleed,
        pos_y + inset - bleed,
        CARD_WIDTH - (2.0 * inset) + (2.0 * bleed),
        CARD_HEIGHT - (2.0 * inset) + (2.0 * bleed),
    )
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

    fn opts(
        cut_lines: CutLines,
        print_layout: PrintLayout,
        thickness: f32,
        upscale: bool,
    ) -> PdfOptions {
        PdfOptions {
            page_size: PageSize::Letter,
            cut_lines,
            print_layout,
            cut_line_thickness: thickness,
            upscale,
            double_sided: false,
            back_label: None,
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
        let base = 0.0;
        assert_eq!(
            opts(
                CutLines::None,
                PrintLayout::EdgeToEdge,
                MAX_CUT_LINE_THICKNESS,
                false,
            )
            .effective_gap(),
            base,
        );
        assert_eq!(
            opts(
                CutLines::Margins,
                PrintLayout::EdgeToEdge,
                MAX_CUT_LINE_THICKNESS,
                false,
            )
            .effective_gap(),
            base,
        );
    }

    #[test]
    fn effective_gap_widens_for_full_page_when_thickness_exceeds_base() {
        let o = opts(CutLines::FullPage, PrintLayout::EdgeToEdge, 3.0, false);
        assert_eq!(o.effective_gap(), 3.0);
    }

    #[test]
    fn effective_gap_preserves_base_for_full_page_when_base_exceeds_thickness() {
        // Gap layout reserves 0.125in (9pt) between cards; a 0.5pt stroke shouldn't shrink it.
        let base = LAYOUT_GAP;
        assert!(base > DEFAULT_CUT_LINE_THICKNESS);
        assert_eq!(
            opts(
                CutLines::FullPage,
                PrintLayout::Gap,
                DEFAULT_CUT_LINE_THICKNESS,
                false,
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
            false,
        );
        let thick = opts(
            CutLines::FullPage,
            PrintLayout::EdgeToEdge,
            MAX_CUT_LINE_THICKNESS,
            false,
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
            false,
        );
        let thick = opts(
            CutLines::Margins,
            PrintLayout::EdgeToEdge,
            MAX_CUT_LINE_THICKNESS,
            false,
        );
        assert_eq!(thin.capacity(), thick.capacity());
    }

    fn layout_opts(
        cut_lines: CutLines,
        page_size: PageSize,
        print_layout: PrintLayout,
        thickness: f32,
    ) -> PdfOptions {
        PdfOptions {
            page_size,
            cut_lines,
            print_layout,
            cut_line_thickness: thickness,
            upscale: false,
            double_sided: false,
            back_label: None,
        }
    }

    fn bleed_opts(cut_lines: CutLines, page_size: PageSize, thickness: f32) -> PdfOptions {
        layout_opts(cut_lines, page_size, PrintLayout::Bleed, thickness)
    }

    #[test]
    fn only_the_bleed_layout_bleeds() {
        for layout in [
            PrintLayout::EdgeToEdge,
            PrintLayout::Gap,
            PrintLayout::Margin,
        ] {
            let o = opts(CutLines::Margins, layout, DEFAULT_CUT_LINE_THICKNESS, false);
            assert_eq!(o.bleed_pt(), 0.0, "{:?} should not bleed", layout);
            assert_eq!(o.bleed_ratio(), 0.0);
        }
        assert!(
            bleed_opts(
                CutLines::Margins,
                PageSize::Letter,
                DEFAULT_CUT_LINE_THICKNESS
            )
            .bleed_pt()
                > 0.0
        );
    }

    #[test]
    fn bleed_never_costs_a_row_or_column() {
        for page_size in [PageSize::Letter, PageSize::A4, PageSize::Custom(5.0, 7.0)] {
            for cut_lines in [CutLines::None, CutLines::Margins, CutLines::FullPage] {
                for thickness in [MIN_CUT_LINE_THICKNESS, DEFAULT_CUT_LINE_THICKNESS, 2.0] {
                    let bled = bleed_opts(cut_lines, page_size, thickness);
                    let plain =
                        layout_opts(cut_lines, page_size, PrintLayout::EdgeToEdge, thickness);

                    assert_eq!(
                        bled.capacity(),
                        plain.capacity(),
                        "{:?} {:?} t={} changed the grid",
                        page_size,
                        cut_lines,
                        thickness,
                    );
                }
            }
        }
    }

    #[test]
    fn no_ink_falls_outside_the_minimum_margin() {
        // The grid plus its outer ring of bleed has to sit inside the reserve.
        for page_size in [PageSize::Letter, PageSize::A4] {
            for cut_lines in [CutLines::None, CutLines::Margins, CutLines::FullPage] {
                let o = bleed_opts(cut_lines, page_size, DEFAULT_CUT_LINE_THICKNESS);
                let (rows, cols) = o.capacity();
                let (left_margin, top_margin) = o.margins();
                let b = o.bleed_pt();
                let gap = o.effective_gap();
                let (page_w, page_h) = page_size.dimensions();

                let ink_left = left_margin - b;
                let ink_right =
                    left_margin + cols as f32 * CARD_WIDTH + (cols - 1) as f32 * gap + b;
                let ink_top = top_margin - b;
                let ink_bottom =
                    top_margin + rows as f32 * CARD_HEIGHT + (rows - 1) as f32 * gap + b;

                assert!(ink_left >= MINIMUM_MARGIN - 0.01, "left {}", ink_left);
                assert!(ink_top >= MINIMUM_MARGIN - 0.01, "top {}", ink_top);
                assert!(
                    ink_right <= page_w - MINIMUM_MARGIN + 0.01,
                    "right {}",
                    ink_right
                );
                assert!(
                    ink_bottom <= page_h - MINIMUM_MARGIN + 0.01,
                    "bottom {}",
                    ink_bottom
                );
            }
        }
    }

    #[test]
    fn bleed_is_capped_at_the_target_and_pinned_per_page() {
        let letter = bleed_opts(
            CutLines::Margins,
            PageSize::Letter,
            DEFAULT_CUT_LINE_THICKNESS,
        );
        let a4 = bleed_opts(CutLines::Margins, PageSize::A4, DEFAULT_CUT_LINE_THICKNESS);

        // Letter is the tight case: 3 rows need 747.27pt of the 756pt printable
        // height, leaving 8.73pt to split over six bleed edges.
        assert!(
            (letter.bleed_mm() - 0.51).abs() < 0.01,
            "letter {}mm",
            letter.bleed_mm()
        );
        // A4 has room for the whole target.
        assert!(
            (a4.bleed_mm() - PDF_BLEED_MM).abs() < 0.001,
            "a4 {}mm",
            a4.bleed_mm()
        );

        assert_eq!(letter.capacity(), (3, 3));
        assert_eq!(a4.capacity(), (3, 3));
    }

    #[test]
    fn the_gap_is_exactly_two_bleeds() {
        // The two neighbours each fill their own half, leaving no white between.
        let o = bleed_opts(
            CutLines::Margins,
            PageSize::Letter,
            DEFAULT_CUT_LINE_THICKNESS,
        );
        assert!((o.effective_gap() - 2.0 * o.bleed_pt()).abs() < 0.001);
    }

    #[test]
    fn margin_cut_marks_pair_up_on_the_card_edges() {
        // Edge to edge, neighbouring card edges coincide and there is one mark per
        // boundary. Under bleed they are 2 * bleed apart, so interior boundaries
        // get two, exactly as they do under the gap layout.
        let plain = opts(
            CutLines::Margins,
            PrintLayout::EdgeToEdge,
            DEFAULT_CUT_LINE_THICKNESS,
            false,
        );
        let bled = bleed_opts(
            CutLines::Margins,
            PageSize::Letter,
            DEFAULT_CUT_LINE_THICKNESS,
        );

        let (rows, cols) = bled.capacity();
        // Column boundaries are marked from the top and the bottom, row boundaries
        // from the left and the right, so each interior boundary gains two marks.
        let interior = (cols - 1) + (rows - 1);
        assert_eq!(
            calculate_margin_cutlines(&bled).len(),
            calculate_margin_cutlines(&plain).len() + 2 * interior
        );

        // And the pair straddles the boundary at the two card edges, 2 * bleed apart.
        let (left_margin, _) = bled.margins();
        let gap = bled.effective_gap();
        let first_card_right = left_margin + CARD_WIDTH;
        let second_card_left = left_margin + CARD_WIDTH + gap;
        assert!((second_card_left - first_card_right - 2.0 * bled.bleed_pt()).abs() < 0.001);
    }

    #[test]
    fn full_page_lines_land_on_the_bleed_edges() {
        let o = bleed_opts(
            CutLines::Margins,
            PageSize::Letter,
            DEFAULT_CUT_LINE_THICKNESS,
        );
        let (left_margin, _) = o.margins();
        let gap = o.effective_gap();
        let b = o.bleed_pt();

        // One line per boundary, unchanged by bleed.
        let plain = opts(
            CutLines::FullPage,
            PrintLayout::EdgeToEdge,
            DEFAULT_CUT_LINE_THICKNESS,
            false,
        );
        assert_eq!(
            calculate_full_page_cutlines(&o).len(),
            calculate_full_page_cutlines(&plain).len()
        );

        // i = 0 sits on the outer edge of the first column's bleed.
        let outer = left_margin + (0.0 * CARD_WIDTH) - (0.5 * gap);
        assert!((outer - (left_margin - b)).abs() < 0.001, "{}", outer);

        // i = 1 sits where card 0's bleed ends and card 1's begins.
        let interior = left_margin + CARD_WIDTH + (0.5 * gap);
        assert!(
            (interior - (left_margin + CARD_WIDTH + b)).abs() < 0.001,
            "{}",
            interior
        );
    }

    #[test]
    fn neighbouring_images_meet_exactly() {
        // The whole point of the layout: card 0's image ends where card 1's begins,
        // so the strip between two cards is bleed all the way across with no seam
        // and no overlap.
        let o = bleed_opts(
            CutLines::Margins,
            PageSize::Letter,
            DEFAULT_CUT_LINE_THICKNESS,
        );
        let (_, cols) = o.capacity();

        let (x0, _, w0, _) = calculate_draw_rect(0, &o);
        let (x1, ..) = calculate_draw_rect(1, &o);
        assert!((x0 + w0 - x1).abs() < 0.001, "{} vs {}", x0 + w0, x1);

        let (_, y0, _, h0) = calculate_draw_rect(0, &o);
        let (_, y_next, ..) = calculate_draw_rect(cols, &o);
        assert!(
            (y0 + h0 - y_next).abs() < 0.001,
            "{} vs {}",
            y0 + h0,
            y_next
        );

        // And the card art still sits `bleed` inside the image it is drawn in.
        let (pos_x, _) = calculate_card_position(0, &o);
        assert!((pos_x - x0 - o.bleed_pt()).abs() < 0.001);
    }

    #[test]
    fn non_bleed_layouts_draw_the_card_rect_unchanged() {
        // Nothing about the existing layouts moves.
        for (layout, inset) in [
            (PrintLayout::EdgeToEdge, 0.0),
            (PrintLayout::Gap, 0.0),
            (PrintLayout::Margin, LAYOUT_INSET),
        ] {
            let o = opts(CutLines::Margins, layout, DEFAULT_CUT_LINE_THICKNESS, false);
            let (pos_x, pos_y) = calculate_card_position(0, &o);
            let (x, y, w, h) = calculate_draw_rect(0, &o);

            assert_eq!((x, y), (pos_x + inset, pos_y + inset), "{:?}", layout);
            assert_eq!(
                (w, h),
                (CARD_WIDTH - 2.0 * inset, CARD_HEIGHT - 2.0 * inset),
                "{:?}",
                layout
            );
        }
    }

    fn duplex(page_size: PageSize, cut_lines: CutLines) -> PdfOptions {
        PdfOptions {
            page_size,
            cut_lines,
            double_sided: true,
            ..PdfOptions::default()
        }
    }

    fn side(key: &str) -> CardSide {
        CardSide {
            image_key: Some(key.to_string()),
            bleed_image_key: None,
        }
    }

    fn collection(key: &str) -> Slot {
        Slot::Collection(SourceImage {
            key: key.to_string(),
            has_bleed: false,
        })
    }

    fn card(backs: &[&str], back_group: &str) -> Printing {
        Printing {
            card_id: "hedge_fund".into(),
            card_title: "Hedge Fund".into(),
            is_official: true,
            variant: None,
            front: side("front.png"),
            backs: backs
                .iter()
                .map(|name| side(&format!("{}.png", name)))
                .collect(),
            collection: "nsg".into(),
            back_group: back_group.into(),
            pack_id: Some("system_gateway".into()),
            date_release: None,
            position: None,
        }
    }

    /// A game id nothing ships backs for, so every fallback lands on Blank.
    const NO_BACKS: &str = "game-with-no-card-backs";

    fn pages_in(printings: &[Printing], options: &PdfOptions, game_id: &str) -> Vec<Page> {
        let (rows, cols) = options.capacity();
        let cards: Vec<PrintedCard> = expand_to_cards(printings)
            .into_iter()
            .map(|(_, card)| card)
            .collect();
        build_pages(
            &cards,
            options,
            game_id,
            BleedPreference::NoBleed,
            rows * cols,
            cols,
        )
    }

    fn pages_for(printings: &[Printing], options: &PdfOptions) -> Vec<Page> {
        pages_in(printings, options, NO_BACKS)
    }

    #[test]
    fn a_mirrored_slot_sits_the_same_distance_from_the_opposite_page_edge() {
        // This is what makes a duplex flip register: the grid is centred, so
        // column c and column cols-1-c are equally far from their own edge.
        for page_size in [PageSize::Letter, PageSize::A4] {
            for cut_lines in [CutLines::None, CutLines::Margins, CutLines::FullPage] {
                for print_layout in [
                    PrintLayout::EdgeToEdge,
                    PrintLayout::Gap,
                    PrintLayout::Bleed,
                ] {
                    let options = PdfOptions {
                        print_layout,
                        ..duplex(page_size, cut_lines)
                    };
                    let (rows, cols) = options.capacity();
                    let (page_width, _) = page_size.dimensions();

                    for index in 0..rows * cols {
                        let (front_x, front_y, width, _) = calculate_draw_rect(index, &options);
                        let (back_x, back_y, back_width, _) =
                            calculate_draw_rect(mirror(index, cols), &options);

                        assert!(
                            (front_x - (page_width - (back_x + back_width))).abs() < 0.001,
                            "{:?} {:?} {:?} slot {}: {} vs {}",
                            page_size,
                            cut_lines,
                            print_layout,
                            index,
                            front_x,
                            page_width - (back_x + back_width),
                        );
                        assert!((front_y - back_y).abs() < 0.001, "slot {} row moved", index);
                        assert!((width - back_width).abs() < 0.001);
                    }
                }
            }
        }
    }

    #[test]
    fn a_centred_grid_needs_no_separate_cut_lines_for_the_back_page() {
        // Cut lines are a function of the options alone, so both pages of a
        // pair carry the same marks. Pinned because duplex depends on it.
        let options = duplex(PageSize::Letter, CutLines::Margins);
        let plain = PdfOptions {
            double_sided: false,
            ..options
        };
        assert_eq!(
            calculate_margin_cutlines(&options).len(),
            calculate_margin_cutlines(&plain).len()
        );
        assert_eq!(
            calculate_full_page_cutlines(&options).len(),
            calculate_full_page_cutlines(&plain).len()
        );
    }

    #[test]
    fn mirroring_stays_within_a_row() {
        assert_eq!(
            (0..6).map(|i| mirror(i, 3)).collect::<Vec<_>>(),
            vec![2, 1, 0, 5, 4, 3]
        );
        assert_eq!(mirror(mirror(4, 3), 3), 4, "mirroring twice is identity");
    }

    #[test]
    fn single_sided_output_leaves_a_backless_card_alone() {
        // Falling back to a standard back here would double every PDF.
        let pages = pages_for(&[card(&[], "corp")], &PdfOptions::default());

        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0], vec![(0, collection("front.png"))]);
    }

    #[test]
    fn single_sided_output_puts_a_cards_own_back_in_the_next_slot() {
        let pages = pages_for(&[card(&["back"], "corp")], &PdfOptions::default());
        assert_eq!(
            pages[0],
            vec![(0, collection("front.png")), (1, collection("back.png"))]
        );
    }

    #[test]
    fn three_copies_of_a_three_back_card_print_six_slots() {
        let melies = card(&["back", "back2", "back3"], "corp");
        let pages = pages_for(&vec![melies; 3], &PdfOptions::default());

        let slots: Vec<&Slot> = pages.iter().flatten().map(|(_, slot)| slot).collect();
        assert_eq!(
            slots,
            vec![
                &collection("front.png"),
                &collection("back.png"),
                &collection("front.png"),
                &collection("back2.png"),
                &collection("front.png"),
                &collection("back3.png"),
            ]
        );
    }

    #[test]
    fn duplex_pairs_every_page_of_fronts_with_a_page_of_backs() {
        let options = duplex(PageSize::Letter, CutLines::Margins);
        let (rows, cols) = options.capacity();
        let per_page = rows * cols;

        let pages = pages_for(&vec![card(&["back"], "corp"); per_page + 1], &options);

        assert_eq!(pages.len(), 4);
        assert_eq!(pages[0].len(), per_page);
        assert_eq!(pages[1].len(), per_page);
        assert_eq!(pages[2].len(), 1);
        assert_eq!(pages[3].len(), 1);
    }

    #[test]
    fn a_partial_page_mirrors_within_the_full_grid() {
        // Five cards on a 3x3 page put their backs at 2, 1, 0, 5, 4 -- the
        // right of the second row stays empty rather than the backs closing up.
        let options = duplex(PageSize::Letter, CutLines::Margins);
        assert_eq!(options.capacity(), (3, 3));

        let pages = pages_for(&vec![card(&["back"], "corp"); 5], &options);
        assert_eq!(
            pages[1].iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            vec![2, 1, 0, 5, 4]
        );
    }

    #[test]
    fn a_back_page_is_emitted_even_when_nothing_on_it_has_a_back() {
        // The printer pairs pages, so dropping a blank back page would put
        // every later front on the wrong side of a sheet.
        let options = duplex(PageSize::Letter, CutLines::Margins);
        let pages = pages_for(&[card(&[], "corp"), card(&[], "runner")], &options);

        assert_eq!(pages.len(), 2);
        assert_eq!(
            pages[1],
            vec![(2, Slot::Blank), (1, Slot::Blank)],
            "this game ships no backs, so there is nothing to fall back on"
        );
    }

    #[test]
    fn duplex_uses_the_games_standard_back_for_cards_without_one() {
        let options = PdfOptions {
            back_label: Some("proxy"),
            ..duplex(PageSize::Letter, CutLines::Margins)
        };
        let pages = pages_in(
            &[card(&[], "corp"), card(&["back"], "runner")],
            &options,
            "netrunner",
        );

        assert_eq!(
            pages[1],
            vec![
                (2, Slot::CardBack("netrunner_corp_proxy.bleed.png", true)),
                (1, collection("back.png")),
            ]
        );
    }

    #[test]
    fn a_game_with_backs_still_blanks_when_no_label_was_chosen() {
        // Callers resolve the label from what the request may use, so a
        // withheld one never arrives here and there is nothing to fall back on.
        let options = duplex(PageSize::Letter, CutLines::Margins);
        let pages = pages_in(&[card(&[], "corp")], &options, "netrunner");

        assert_eq!(pages[1][0].1, Slot::Blank);
    }

    #[test]
    fn a_card_backs_bleed_comes_from_its_file_name() {
        // A back that carries its own bleed says so with `.bleed`, so it is
        // cropped to the layout rather than having one added. A back without
        // the suffix takes the generated path, like any other image.
        assert!(Slot::CardBack("netrunner_corp_proxy.bleed.png", true).has_bleed());
        assert!(!Slot::CardBack("some_flat_back.png", false).has_bleed());
        assert!(!collection("front.png").has_bleed());
    }

    #[test]
    fn a_standard_back_is_never_upscaled() {
        // They already ship at print resolution; upscaling only inflates the PDF.
        assert!(!Slot::CardBack("netrunner_corp_proxy.bleed.png", true).upscalable());
        assert!(collection("front.png").upscalable());
    }

    #[test]
    fn a_blank_slot_loads_nothing() {
        assert_eq!(Slot::Blank.cache_key(), None);
        assert!(
            Slot::CardBack("netrunner_corp_proxy.bleed.png", true)
                .cache_key()
                .is_some()
        );
    }

    #[test]
    fn a_side_with_no_image_still_takes_a_slot() {
        // Duplex indexes backs by position, so dropping the slot would shift
        // every later card onto the wrong reverse.
        let mut bare = card(&["back"], "corp");
        bare.front = CardSide::default();

        let pages = pages_for(&[bare], &duplex(PageSize::Letter, CutLines::Margins));
        assert_eq!(pages[0], vec![(0, Slot::Blank)]);
        assert_eq!(pages[1], vec![(2, collection("back.png"))]);
    }

    #[test]
    fn bleed_and_duplex_compose() {
        // Backs are laid out on the same grid as fronts, so the property that
        // neighbouring images meet exactly holds on a back page too.
        let options = PdfOptions {
            print_layout: PrintLayout::Bleed,
            ..duplex(PageSize::Letter, CutLines::Margins)
        };
        let (_, cols) = options.capacity();

        // Slots 1 and 0 are neighbours on the back page, in that order.
        let (x_left, _, width, _) = calculate_draw_rect(mirror(1, cols), &options);
        let (x_right, ..) = calculate_draw_rect(mirror(0, cols), &options);
        assert!(
            (x_left + width - x_right).abs() < 0.001,
            "{} vs {}",
            x_left + width,
            x_right
        );
    }

    #[test]
    fn a_page_too_small_for_a_card_produces_no_pages() {
        let options = PdfOptions {
            page_size: PageSize::Custom(1.0, 1.0),
            ..PdfOptions::default()
        };
        assert_eq!(options.capacity(), (0, 0));
        assert!(pages_for(&[card(&["back"], "corp")], &options).is_empty());
    }
}
