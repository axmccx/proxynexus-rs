use clap::{Parser, Subcommand};
use proxynexus_core::collection_builder::CollectionBuilder;
use proxynexus_core::collection_manager::CollectionManager;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "proxynexus-cli")]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short = 'd', long = "verbose", global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate {
        #[command(subcommand)]
        output_type: GenerateType,
    },

    Collection {
        #[command(subcommand)]
        action: CollectionAction,
    },
}

#[derive(Subcommand)]
enum CollectionAction {
    Build {
        #[arg(short, long)]
        images: PathBuf,

        #[arg(short, long)]
        metadata: PathBuf,

        #[arg(short, long)]
        output: PathBuf,

        #[arg(short, long, default_value = "en")]
        language: String,

        #[arg(short, long, default_value = "1.0.0")]
        version: String,
    },

    Add {
        path: PathBuf,
    },

    List,

    Remove {
        name: String,
    },
}

#[derive(Subcommand)]
enum GenerateType {
    Pdf {
        #[arg(short, long)]
        collections: String,

        #[arg(short = 'i', long)]
        card_ids: String,

        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collection { action } => match action {
            CollectionAction::Build {
                output,
                images,
                metadata,
                language,
                version,
            } => {
                handle_collection_build(output, images, metadata, language, version, cli.verbose);
            }
            CollectionAction::Add { path } => handle_collection_add(path),
            CollectionAction::List => handle_collection_list(),
            CollectionAction::Remove { name } => handle_collection_remove(name),
        },

        Commands::Generate { output_type } => match output_type {
            GenerateType::Pdf {
                collections,
                card_ids,
                output,
            } => {
                handle_generate_pdf(collections, card_ids, output);
            }
        },
    }
}

fn handle_collection_build(
    output: PathBuf,
    images: PathBuf,
    metadata: PathBuf,
    language: String,
    version: String,
    verbose: bool,
) {
    match CollectionBuilder::new(output, images, metadata, language, version)
        .verbose(verbose)
        .build()
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Build failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn handle_collection_add(path: PathBuf) {
    match CollectionManager::new() {
        Ok(manager) => {
            if let Err(e) = manager.add_collection(&path) {
                eprintln!("Failed to add collection: {}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to initialize collection manager: {}", e);
            std::process::exit(1);
        }
    }
}

fn handle_collection_list() {
    match CollectionManager::new() {
        Ok(manager) => match manager.get_collections() {
            Ok(collections) => {
                if collections.is_empty() {
                    println!(
                        "No collections available. Use 'collection add <file.pnx>' to add one."
                    );
                } else {
                    println!("Available collections:");
                    for collection in &collections {
                        let (name, version, language) = collection;
                        println!("  {} (v{}, {})", name, version, language);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to list collections: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Failed to initialize collection manager: {}", e);
            std::process::exit(1);
        }
    }
}

fn handle_collection_remove(name: String) {
    println!("Removing collection: {}", name);
    todo!("Implement collection remove");
}

fn handle_generate_pdf(collections: String, card_ids: String, output: PathBuf) {
    println!("Generating {:?} from collections: {}", output, collections);
    println!("Using card_ids: {}", card_ids);
    println!("Generating PDF from collections: {}", collections);
    // match proxynexus_core::create_pdf_with_images(images_path, &args.output_path, PageSize::Letter) {
    //     Ok(_) => println!("PDF created successfully: {:?}", &args.output_path),
    //     Err(e) => eprintln!("Error: {}", e),
    // }
    todo!("Implement PDF generation");
}
