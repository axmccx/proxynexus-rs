use crate::error::{ProxyNexusError, Result};
use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage, imageops::FilterType};

const CUT_WIDTH: f32 = 744.0;
const CUT_HEIGHT: f32 = 1038.0;
const BLEED_WIDTH: f32 = 816.0;
const BLEED_HEIGHT: f32 = 1110.0;

#[derive(Debug, Clone)]
struct BleedConfig {
    output_width: u32,
    output_height: u32,
    bleed_x: u32,
    bleed_y: u32,
}

impl BleedConfig {
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

pub fn add_bleed_border(img: &DynamicImage) -> RgbImage {
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

    let src_raw = working_img.as_raw();
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

// changes a few pixels near top left corner, based on position.
// makes the duplicate image unique, so that MPC doesn't deduplicate it on upload
pub fn apply_uniqueness_marker(img: &mut RgbImage, position: u32) {
    let r_add = ((position * 73) % 256) as u8;
    let g_add = ((position * 137) % 256) as u8;
    let b_add = ((position * 193) % 256) as u8;

    for y in 0..2 {
        for x in 0..2 {
            if x < img.width() && y < img.height() {
                let pixel = img.get_pixel_mut(x, y);
                pixel.0[0] = pixel.0[0].wrapping_add(r_add);
                pixel.0[1] = pixel.0[1].wrapping_add(g_add);
                pixel.0[2] = pixel.0[2].wrapping_add(b_add);
            }
        }
    }
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

    #[test]
    fn test_uniqueness_marker_bounds() {
        let mut img = RgbImage::new(10, 10);
        apply_uniqueness_marker(&mut img, 0);
        apply_uniqueness_marker(&mut img, 5);
        apply_uniqueness_marker(&mut img, 100);
    }

    #[test]
    fn test_apply_uniqueness_marker_hashes() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut img1 = RgbImage::new(100, 100);
        for p in img1.pixels_mut() {
            *p = image::Rgb([255, 255, 255]);
        }
        let mut img2 = img1.clone();

        apply_uniqueness_marker(&mut img1, 1);
        apply_uniqueness_marker(&mut img2, 2);

        fn hash_img(img: &RgbImage) -> u64 {
            let mut hasher = DefaultHasher::new();
            img.as_raw().hash(&mut hasher);
            hasher.finish()
        }

        assert_ne!(hash_img(&img1), hash_img(&img2));
    }

    #[test]
    fn test_add_bleed_border() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(744, 1038));
        let bordered = add_bleed_border(&img);

        assert_eq!(bordered.width(), 816);
        assert_eq!(bordered.height(), 1110);
    }
}
