use opencv::{
    core::{BORDER_REPLICATE, Mat, Scalar, Vec3b, Vector, copy_make_border},
    imgcodecs,
    prelude::*,
};

#[derive(Debug, Clone)]
struct BorderConfig {
    bleed_width: u32,
    bleed_height: u32,
}

impl BorderConfig {
    /// Calculate border parameters dynamically based on input image dimensions
    /// MPC requires 36px bleed per side at 300 DPI (744px width baseline)
    /// Scales proportionally for any resolution
    fn from_image_dimensions(width: u32, height: u32) -> Self {
        let dpi_scale = width as f32 / 744.0;

        let bleed_pixels_per_side = (36.0 * dpi_scale).round() as u32;

        let bleed_width = width + (bleed_pixels_per_side * 2);
        let bleed_height = height + (bleed_pixels_per_side * 2);

        Self {
            bleed_width,
            bleed_height,
        }
    }
}

pub fn generate_bordered_image(
    img: &Mat,
    marker_position: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let width = img.cols() as u32;
    let height = img.rows() as u32;

    let config = BorderConfig::from_image_dimensions(width, height);
    let mut bordered = replicate_edges_to_bleed(img, &config)?;

    apply_uniqueness_marker(&mut bordered, marker_position)?;

    let mut buf = Vector::new();
    imgcodecs::imencode(".jpg", &bordered, &mut buf, &Vector::new())?;

    Ok(buf.to_vec())
}

fn replicate_edges_to_bleed(
    img: &Mat,
    config: &BorderConfig,
) -> Result<Mat, Box<dyn std::error::Error>> {
    let height = img.rows() as u32;
    let width = img.cols() as u32;

    let border_width = (config.bleed_width.saturating_sub(width)) / 2;
    let border_height = (config.bleed_height.saturating_sub(height)) / 2;

    if config.bleed_width < width || config.bleed_height < height {
        return Err(format!(
            "Image {}×{} is already larger than bleed size {}×{}",
            width, height, config.bleed_width, config.bleed_height
        )
        .into());
    }

    let mut dst = Mat::default();

    copy_make_border(
        img,
        &mut dst,
        border_height as i32,
        border_height as i32,
        border_width as i32,
        border_width as i32,
        BORDER_REPLICATE,
        Scalar::default(),
    )?;

    Ok(dst)
}

// changes a few pixels near top left corner, based on position.
// makes the image duplicate image unique, so that MPC doesn't deduplicate it on upload
fn apply_uniqueness_marker(img: &mut Mat, position: u32) -> opencv::Result<()> {
    let x = position as i32;
    let y = 0;

    let pixel = img.at_2d_mut::<Vec3b>(y, x)?;

    pixel[2] = pixel[2].saturating_add((position * 10) as u8);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_border_config_calculation() {
        // Test with NSG-sized image (744×1039)
        // Should add 36px per side at 300 DPI scale
        let config = BorderConfig::from_image_dimensions(744, 1039);
        assert_eq!(config.bleed_width, 816); // 744 + (36*2)
        assert_eq!(config.bleed_height, 1111); // 1039 + (36*2)

        // Test with PopTartNZ-sized image (1461×2076)
        // DPI scale ≈ 1.96, so 36px scales to ~71px per side
        let config = BorderConfig::from_image_dimensions(1461, 2076);
        let expected_bleed_per_side = (36.0 * (1461.0 / 744.0) as f32).round() as u32;
        assert_eq!(config.bleed_width, 1461 + (expected_bleed_per_side * 2));
        assert_eq!(config.bleed_height, 2076 + (expected_bleed_per_side * 2));
    }
}
