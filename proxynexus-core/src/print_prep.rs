use crate::error::{ProxyNexusError, Result};
use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage, imageops::FilterType};

const CUT_WIDTH: f32 = 744.0;
const CUT_HEIGHT: f32 = 1038.0;
const BLEED_WIDTH: f32 = 816.0;
const BLEED_HEIGHT: f32 = 1110.0;
const MAX_IMAGE_WIDTH: f32 = 2176.0;
const MAX_IMAGE_HEIGHT: f32 = 2960.0;

#[derive(Debug, Clone)]
struct BleedConfig {
    output_width: u32,
    output_height: u32,
    bleed_x: u32,
    bleed_y: u32,
}

impl BleedConfig {
    fn uniform(width: u32, height: u32, bleed: u32) -> Self {
        Self {
            output_width: width + 2 * bleed,
            output_height: height + 2 * bleed,
            bleed_x: bleed,
            bleed_y: bleed,
        }
    }

    /// Calculate dimensions and bleed size based on the longest side of the input image.
    /// Scales proportionally for any resolution
    fn calculate(width: u32, height: u32) -> Self {
        let scale = (width as f32 / CUT_WIDTH).max(height as f32 / CUT_HEIGHT);
        let output_width = (BLEED_WIDTH * scale).round() as u32;
        let output_height = (BLEED_HEIGHT * scale).round() as u32;

        Self {
            output_width,
            output_height,
            bleed_x: (output_width - width) / 2,
            bleed_y: (output_height - height) / 2,
        }
    }
}

pub fn crop_bleed_border(img: &DynamicImage, keep_ratio: f32) -> DynamicImage {
    let width = img.width();
    let height = img.height();

    let keep = (width as f32 * CUT_WIDTH / BLEED_WIDTH) * keep_ratio;

    let full_x = width as f32 * (BLEED_WIDTH - CUT_WIDTH) / 2.0 / BLEED_WIDTH;
    let full_y = height as f32 * (BLEED_HEIGHT - CUT_HEIGHT) / 2.0 / BLEED_HEIGHT;

    let crop_x = (full_x - keep).round().max(0.0) as u32;
    let crop_y = (full_y - keep).round().max(0.0) as u32;

    img.crop_imm(
        crop_x,
        crop_y,
        width.saturating_sub(crop_x * 2),
        height.saturating_sub(crop_y * 2),
    )
}

pub fn add_uniform_bleed_border(img: &DynamicImage, bleed_ratio: f32) -> RgbImage {
    let src = img.to_rgb8();
    let (src_w, src_h) = src.dimensions();
    let bleed = (src_w as f32 * bleed_ratio).round().max(0.0) as u32;
    let config = BleedConfig::uniform(src_w, src_h, bleed);

    generate_bleed(&src, &config)
}

pub fn add_mpc_bleed_border(img: &DynamicImage) -> RgbImage {
    let (orig_w, orig_h) = img.dimensions();

    // If image is smaller than the mpc cutline, scale so the longest side fits.
    let scale_to_fit = (CUT_WIDTH / orig_w as f32).min(CUT_HEIGHT / orig_h as f32);
    let working_img = if scale_to_fit > 1.0 {
        let new_w = (orig_w as f32 * scale_to_fit).round() as u32;
        let new_h = (orig_h as f32 * scale_to_fit).round() as u32;
        let scaled = img.resize_exact(new_w, new_h, FilterType::Lanczos3);
        scaled.to_rgb8()
    } else {
        img.to_rgb8()
    };

    let (src_w, src_h) = working_img.dimensions();
    let config = BleedConfig::calculate(src_w, src_h);

    generate_bleed(&working_img, &config)
}

pub fn max_upscale_size(has_bleed: bool) -> (u32, u32) {
    if has_bleed {
        (MAX_IMAGE_WIDTH as u32, MAX_IMAGE_HEIGHT as u32)
    } else {
        (
            (MAX_IMAGE_WIDTH * CUT_WIDTH / BLEED_WIDTH).round() as u32,
            (MAX_IMAGE_HEIGHT * CUT_HEIGHT / BLEED_HEIGHT).round() as u32,
        )
    }
}

fn generate_bleed(src: &RgbImage, config: &BleedConfig) -> RgbImage {
    let (src_w, src_h) = src.dimensions();
    let src_raw = src.as_raw();
    let mut dest_raw = vec![0u8; (config.output_width * config.output_height * 3) as usize];

    for y in 0..config.output_height {
        let src_y = (y as i32 - config.bleed_y as i32).clamp(0, src_h as i32 - 1) as u32;
        let src_row_start = (src_y * src_w * 3) as usize;
        let src_row_end = src_row_start + (src_w * 3) as usize;
        let src_row = &src_raw[src_row_start..src_row_end];

        let dest_row_start = (y * config.output_width * 3) as usize;

        // 1. Fill Left Border (Repeat the first pixel of the source row)
        let first_pixel = &src_row[0..3];
        for x in 0..config.bleed_x {
            let idx = dest_row_start + (x * 3) as usize;
            dest_raw[idx..idx + 3].copy_from_slice(first_pixel);
        }

        // 2. Fill Center (Fast blit of the entire source row)
        let center_start = dest_row_start + (config.bleed_x * 3) as usize;
        let center_end = center_start + (src_w * 3) as usize;
        dest_raw[center_start..center_end].copy_from_slice(src_row);

        // 3. Fill Right Border (Repeat the last pixel of the source row)
        let last_pixel = &src_row[(src_w as usize - 1) * 3..];
        for x in (config.bleed_x + src_w)..config.output_width {
            let idx = dest_row_start + (x * 3) as usize;
            dest_raw[idx..idx + 3].copy_from_slice(last_pixel);
        }
    }

    image::ImageBuffer::from_raw(config.output_width, config.output_height, dest_raw).unwrap()
}

pub fn encode_image(bordered: RgbImage, format: ImageFormat) -> Result<Vec<u8>> {
    if format == ImageFormat::Png {
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(bordered).write_to(&mut png_bytes, ImageFormat::Png)?;
        return Ok(png_bytes.into_inner());
    }

    let mut jpeg_bytes = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut jpeg_bytes, 95);

    encoder
        .encode(
            bordered.as_raw(),
            bordered.width() as u16,
            bordered.height() as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|e| ProxyNexusError::Internal(e.to_string()))?;

    Ok(jpeg_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ART: image::Rgb<u8> = image::Rgb([10, 200, 30]);
    const BLEED: image::Rgb<u8> = image::Rgb([200, 10, 30]);

    /// A source whose outer ring is BLEED and whose interior is ART, so the two
    /// are distinguishable in the prepared output. `inset` of 0 makes it a plain
    /// unbled card image.
    fn source(w: u32, h: u32, inset_x: u32, inset_y: u32) -> DynamicImage {
        let mut img = RgbImage::from_pixel(w, h, BLEED);
        for y in inset_y..h - inset_y {
            for x in inset_x..w - inset_x {
                img.put_pixel(x, y, ART);
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    /// 0.51mm of bleed against a 62.985mm card, the Letter case.
    const LETTER_RATIO: f32 = 0.0081;

    #[test]
    fn crop_keeps_the_requested_amount_of_the_sources_bleed() {
        // 816x1110 carries the full 36px bleed around a 744x1038 card. 0.0081 of
        // 744 is 6px, so 30px comes off each side and 6px of real bleed stays.
        let img = source(816, 1110, 36, 36);
        let out = crop_bleed_border(&img, LETTER_RATIO).to_rgb8();

        assert_eq!(out.dimensions(), (744 + 12, 1038 + 12));

        let (w, h) = out.dimensions();
        // Last bleed pixel, then first art pixel, on all four sides.
        assert_eq!(*out.get_pixel(5, h / 2), BLEED);
        assert_eq!(*out.get_pixel(6, h / 2), ART);
        assert_eq!(*out.get_pixel(w - 6, h / 2), BLEED);
        assert_eq!(*out.get_pixel(w - 7, h / 2), ART);
        assert_eq!(*out.get_pixel(w / 2, 5), BLEED);
        assert_eq!(*out.get_pixel(w / 2, 6), ART);
        assert_eq!(*out.get_pixel(w / 2, h - 6), BLEED);
        assert_eq!(*out.get_pixel(w / 2, h - 7), ART);
    }

    #[test]
    fn crop_with_no_keep_strips_the_bleed_entirely() {
        // What the layouts that want no bleed get, and what this function did
        // before it took a ratio at all.
        let img = source(816, 1110, 36, 36);
        let out = crop_bleed_border(&img, 0.0).to_rgb8();

        assert_eq!(out.dimensions(), (744, 1038));
        assert_eq!(*out.get_pixel(0, 0), ART);
        assert_eq!(*out.get_pixel(743, 1037), ART);
    }

    #[test]
    fn crop_stops_at_zero_when_the_source_is_short_on_bleed() {
        // Asking for more than the source carries leaves it uncropped rather than
        // growing it. Cannot happen with MPC-ratio sources, whose 0.048 is three
        // times the largest PDF bleed.
        let img = source(816, 1110, 36, 36);
        let out = crop_bleed_border(&img, 0.2);

        assert_eq!(out.dimensions(), (816, 1110));
    }

    #[test]
    fn crop_scales_with_source_resolution() {
        // Twice the resolution, twice the pixels kept: the physical bleed left
        // behind is the same, which is what keeps it aligned with the cut lines.
        let single = crop_bleed_border(&source(816, 1110, 36, 36), LETTER_RATIO);
        let double = crop_bleed_border(&source(1632, 2220, 72, 72), LETTER_RATIO);

        assert_eq!(single.width() - 744, 12);
        assert_eq!(double.width() - 1488, 24);
    }

    #[test]
    fn uniform_bleed_repeats_the_edge_pixels() {
        let img = source(744, 1038, 0, 0);
        let out = add_uniform_bleed_border(&img, LETTER_RATIO);

        assert_eq!(out.dimensions(), (744 + 12, 1038 + 12));
        assert_eq!(*out.get_pixel(0, out.height() / 2), ART);
        assert_eq!(*out.get_pixel(out.width() - 1, out.height() / 2), ART);
        assert_eq!(*out.get_pixel(out.width() / 2, 0), ART);
        assert_eq!(*out.get_pixel(out.width() / 2, out.height() - 1), ART);
    }

    #[test]
    fn uniform_bleed_scales_with_source_resolution() {
        assert_eq!(
            add_uniform_bleed_border(&source(744, 1038, 0, 0), LETTER_RATIO).width() - 744,
            12
        );
        assert_eq!(
            add_uniform_bleed_border(&source(1488, 2076, 0, 0), LETTER_RATIO).width() - 1488,
            24
        );
    }

    #[test]
    fn both_paths_leave_the_card_the_same_size() {
        // The point of the pair: whichever a source needs, the art comes out at the
        // cut size with `bleed` around it, so the cut lines land in the same place.
        let cropped = crop_bleed_border(&source(816, 1110, 36, 36), LETTER_RATIO);
        let generated = add_uniform_bleed_border(&source(744, 1038, 0, 0), LETTER_RATIO);

        assert_eq!(cropped.dimensions(), generated.dimensions());
    }

    #[test]
    fn tiny_sources_do_not_panic() {
        let tiny = DynamicImage::ImageRgb8(RgbImage::from_pixel(3, 4, ART));
        assert!(crop_bleed_border(&tiny, LETTER_RATIO).width() > 0);
        assert!(add_uniform_bleed_border(&tiny, LETTER_RATIO).width() > 0);
    }

    #[test]
    fn test_border_config_calculation() {
        // Test with standard size
        let config = BleedConfig::calculate(744, 1038);
        assert_eq!(config.output_width, 816);
        assert_eq!(config.output_height, 1110);
        assert_eq!(config.bleed_x, 36);
        assert_eq!(config.bleed_y, 36);

        // Test with large PopTartNZ image
        let config = BleedConfig::calculate(1461, 2076);
        assert_eq!(config.output_width, 1632);
        assert_eq!(config.output_height, 2220);
        assert_eq!(config.bleed_x, 85);
        assert_eq!(config.bleed_y, 72);

        // Following tests aren't expected in practice,
        // because the image should be scaled to baseline before calculating BleedConfig

        // Test with slightly smaller NSG image
        let config = BleedConfig::calculate(744, 1031);
        assert_eq!(config.output_width, 816);
        assert_eq!(config.output_height, 1110);
        assert_eq!(config.bleed_x, 36);
        assert_eq!(config.bleed_y, 39);

        // Test with smallest NSG image
        let config = BleedConfig::calculate(481, 669);
        assert_eq!(config.output_width, 528);
        assert_eq!(config.output_height, 718);
        assert_eq!(config.bleed_x, 23);
        assert_eq!(config.bleed_y, 24);
    }

    /// What `cap_to` in the upscaler does with the size this hands it.
    fn cap(w: u32, h: u32, max: (u32, u32)) -> (u32, u32) {
        let scale = (max.0 as f32 / w as f32).min(max.1 as f32 / h as f32);
        if scale >= 1.0 {
            return (w, h);
        }
        (
            (w as f32 * scale).round() as u32,
            (h as f32 * scale).round() as u32,
        )
    }

    #[test]
    fn a_bled_source_is_capped_at_the_full_bleed_size() {
        assert_eq!(max_upscale_size(true), (2176, 2960));
    }

    #[test]
    fn an_unbled_source_is_capped_at_that_outputs_cut_area() {
        // 2176 * 744/816 and 2960 * 1038/1110, both exact. Bleed is generated
        // after upscaling, which grows it back to the full size above.
        assert_eq!(max_upscale_size(false), (1984, 2768));
    }

    #[test]
    fn both_source_kinds_reach_the_same_card_resolution() {
        // The point of the pair. A bled source passes through at its capped
        // size; an unbled one has bleed added, and the two land together.
        let bled = cap(1568 * 4, 2140 * 4, max_upscale_size(true));
        assert_eq!(bled, (2169, 2960));

        let unbled = cap(744 * 4, 1038 * 4, max_upscale_size(false));
        assert_eq!(unbled, (1984, 2768));
        let generated = add_mpc_bleed_border(&source(unbled.0, unbled.1, 0, 0));
        assert_eq!(generated.dimensions(), (2176, 2960));

        // Same card, within a third of a percent either way.
        let ratio = generated.width() as f32 / bled.0 as f32;
        assert!(ratio > 0.99 && ratio < 1.01, "{ratio}");
    }

    #[test]
    fn a_source_that_already_fits_is_left_alone() {
        // The library's own sizes, which the upscaler reads rather than writes.
        assert_eq!(cap(1568, 2140, max_upscale_size(true)), (1568, 2140));
        assert_eq!(cap(1632, 2220, max_upscale_size(true)), (1632, 2220));
    }

    #[test]
    fn test_add_mpc_bleed_border() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(744, 1038));
        let bordered = add_mpc_bleed_border(&img);

        assert_eq!(bordered.width(), 816);
        assert_eq!(bordered.height(), 1110);
    }
}
