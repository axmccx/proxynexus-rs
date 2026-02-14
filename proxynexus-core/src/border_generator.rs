use opencv::{
    core::{BORDER_REPLICATE, Mat, Scalar, copy_make_border},
    imgcodecs,
    prelude::*,
};
use std::path::Path;

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

pub fn generate_mpc_border(
    input_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let img = imgcodecs::imread(
        input_path.to_str().ok_or("Invalid input path encoding")?,
        imgcodecs::IMREAD_COLOR,
    )?;

    let width = img.cols() as u32;
    let height = img.rows() as u32;

    let config = BorderConfig::from_image_dimensions(width, height);
    let bordered = replicate_edges_to_bleed(&img, &config)?;

    imgcodecs::imwrite(
        output_path.to_str().ok_or("Invalid output path encoding")?,
        &bordered,
        &opencv::core::Vector::new(),
    )?;

    Ok(())
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
