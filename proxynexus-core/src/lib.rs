use krilla::Data;
use krilla::Document;
use krilla::geom::Size;
use krilla::image::Image;
use krilla::page::PageSettings;
use std::path::Path;

pub fn create_pdf_with_image(
    image_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();

    let page_settings = PageSettings::from_wh(612.0, 792.0);
    let mut page = document.start_page_with(page_settings.unwrap());

    let mut surface = page.surface();

    let image_data = std::fs::read(image_path)?;
    let data = Data::from(image_data);

    let image = Image::from_jpeg(data, true)?;
    let size = Size::from_wh(178.0, 249.0);

    surface.draw_image(image, size.expect("REASON"));

    surface.finish();
    page.finish();

    let pdf = document.finish().unwrap();
    std::fs::write(output_path, &pdf)?;

    Ok(())
}
