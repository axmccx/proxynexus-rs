use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    image_path: String,

    #[arg(short, long, default_value_t = String::from("output.pdf"))]
    output_path: String,
}

fn main() {
    let args = Args::parse();

    let image_path = PathBuf::from(&args.image_path);
    let output_path = PathBuf::from(&args.output_path);

    println!("Adding {} to {}!", args.image_path, args.output_path);

    match proxynexus_core::create_pdf_with_image(&image_path, &output_path) {
        Ok(_) => println!("PDF created successfully: {}", &args.output_path),
        Err(e) => eprintln!("Error: {}", e),
    }
}
