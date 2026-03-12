use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage};

#[derive(Debug, Clone)]
struct BorderConfig {
    output_width: u32,
    output_height: u32,
    border_size: u32,
}

impl BorderConfig {
    /// Calculate border parameters dynamically based on input image dimensions
    /// MPC requires 36px bleed per side at 300 DPI (744px width baseline)
    /// Scales proportionally for any resolution
    fn from_image_dimensions(width: u32, height: u32) -> Self {
        let dpi_scale = width as f32 / 744.0;
        let border_size = (36.0 * dpi_scale).round() as u32;

        Self {
            output_width: width + (border_size * 2),
            output_height: height + (border_size * 2),
            border_size,
        }
    }
}

pub fn generate_bordered_image(
    img: &DynamicImage,
    marker_position: u32,
    format: ImageFormat,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (width, height) = img.dimensions();
    let config = BorderConfig::from_image_dimensions(width, height);

    let mut bordered = replicate_edges_to_bleed(img, &config);
    apply_uniqueness_marker(&mut bordered, marker_position);

    if format == ImageFormat::Png {
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(bordered).write_to(&mut png_bytes, ImageFormat::Png)?;
        return Ok(png_bytes.into_inner());
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(turbojpeg::compress_image(&bordered, 95, turbojpeg::Subsamp::Sub2x2)?.to_vec())
    }

    #[cfg(target_arch = "wasm32")]
    {
        let mut jpeg_bytes = Vec::new();
        let encoder = jpeg_encoder::Encoder::new(&mut jpeg_bytes, 95);

        encoder.encode(
            bordered.as_raw(),
            bordered.width() as u16,
            bordered.height() as u16,
            jpeg_encoder::ColorType::Rgb,
        )?;

        Ok(jpeg_bytes)
    }
}

fn replicate_edges_to_bleed(img: &DynamicImage, config: &BorderConfig) -> RgbImage {
    let (src_w, src_h) = img.dimensions();
    let rgb_view = img.to_rgb8();
    let src_raw = rgb_view.as_raw();

    let mut dest_raw = vec![0u8; (config.output_width * config.output_height * 3) as usize];

    for y in 0..config.output_height {
        let src_y = (y as i32 - config.border_size as i32).clamp(0, src_h as i32 - 1) as u32;
        let src_row_start = (src_y * src_w * 3) as usize;
        let src_row_end = src_row_start + (src_w * 3) as usize;
        let src_row = &src_raw[src_row_start..src_row_end];

        let dest_row_start = (y * config.output_width * 3) as usize;

        // 1. Fill Left Border (Repeat the first pixel of the source row)
        let first_pixel = &src_row[0..3];
        for x in 0..config.border_size {
            let idx = dest_row_start + (x * 3) as usize;
            dest_raw[idx..idx + 3].copy_from_slice(first_pixel);
        }

        // 2. Fill Center (Fast blit of the entire source row)
        let center_start = dest_row_start + (config.border_size * 3) as usize;
        let center_end = center_start + (src_w * 3) as usize;
        dest_raw[center_start..center_end].copy_from_slice(src_row);

        // 3. Fill Right Border (Repeat the last pixel of the source row)
        let last_pixel = &src_row[(src_w as usize - 1) * 3..];
        for x in (config.border_size + src_w)..config.output_width {
            let idx = dest_row_start + (x * 3) as usize;
            dest_raw[idx..idx + 3].copy_from_slice(last_pixel);
        }
    }

    image::ImageBuffer::from_raw(config.output_width, config.output_height, dest_raw).unwrap()
}

// changes a few pixels near top left corner, based on position.
// makes the duplicate image unique, so that MPC doesn't deduplicate it on upload
fn apply_uniqueness_marker(img: &mut RgbImage, position: u32) {
    if position == 0 {
        return;
    }

    let x = position;
    if x < img.width() {
        let pixel = img.get_pixel_mut(x, 0);
        pixel.0[0] = pixel.0[0].saturating_add((position * 10) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_border_config_calculation() {
        // Test with NSG-sized image (744×1039)
        // Should add 36px per side at 300 DPI scale
        let config = BorderConfig::from_image_dimensions(744, 1039);
        assert_eq!(config.output_width, 816); // 744 + (36*2)
        assert_eq!(config.output_height, 1111); // 1039 + (36*2)

        // Test with PopTartNZ-sized image (1461×2076)
        // DPI scale ≈ 1.96, so 36px scales to ~71px per side
        let config = BorderConfig::from_image_dimensions(1461, 2076);
        let expected_bleed_per_side = (36.0 * (1461.0 / 744.0_f32)).round() as u32;
        assert_eq!(config.output_width, 1461 + (expected_bleed_per_side * 2));
        assert_eq!(config.output_height, 2076 + (expected_bleed_per_side * 2));
    }

    #[test]
    fn test_uniqueness_marker_bounds() {
        let mut img = RgbImage::new(10, 10);
        apply_uniqueness_marker(&mut img, 0);
        apply_uniqueness_marker(&mut img, 5);
        apply_uniqueness_marker(&mut img, 100);
    }
}
