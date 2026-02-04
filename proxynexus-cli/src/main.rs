use std::path::PathBuf;

fn main() {
    let image_path = PathBuf::from("images/sample.jpg");
    let output_path = PathBuf::from("output.pdf");

    match proxynexus_core::create_pdf_with_image(&image_path, &output_path) {
        Ok(_) => println!("PDF created successfully: output.pdf"),
        Err(e) => eprintln!("Error: {}", e),
    }
}
