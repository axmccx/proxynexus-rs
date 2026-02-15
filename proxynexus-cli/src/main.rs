use clap::{Parser, Subcommand};
use proxynexus_core::collection_builder::CollectionBuilder;
use proxynexus_core::collection_manager::CollectionManager;
use proxynexus_core::mpc::{generate_mpc_zip_from_cardlist, generate_mpc_zip_from_set_name};
use proxynexus_core::pdf::{PageSize, generate_pdf_from_cardlist, generate_pdf_from_set_name};
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
    #[command(group(
        clap::ArgGroup::new("input")
            .required(true)
            .args(["cardlist", "set_name"]),
    ))]
    Pdf {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(short, long, default_value = "output.pdf")]
        output_path: PathBuf,
    },
    Mpc {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(short, long, default_value = "output.zip")]
        output_path: PathBuf,
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
                cardlist,
                set_name,
                output_path,
            } => {
                if let Some(list) = cardlist {
                    handle_generate_pdf_from_cardlist(list, output_path);
                } else if let Some(name) = set_name {
                    handle_generate_pdf_from_set_name(name, output_path);
                }
            }
            GenerateType::Mpc {
                cardlist,
                set_name,
                output_path,
            } => {
                if let Some(list) = cardlist {
                    handle_generate_mpc_zip_from_cardlist(list, output_path);
                } else if let Some(name) = set_name {
                    handle_generate_mpc_zip_from_set_name(name, output_path);
                }
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
    println!(
        "Are you sure you want to remove collection '{}'? (y/N)",
        name
    );

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    if input.trim().to_lowercase() != "y" {
        return;
    }

    match CollectionManager::new() {
        Ok(manager) => match manager.remove_collection(&name) {
            Ok(_) => {
                println!("Collection '{}' removed successfully.", name);
            }
            Err(e) => {
                eprintln!("Failed to remove collection: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Failed to initialize collection manager: {}", e);
            std::process::exit(1);
        }
    }
}

fn handle_generate_pdf_from_cardlist(cardlist: String, output_path: PathBuf) {
    match generate_pdf_from_cardlist(&cardlist, &output_path, PageSize::Letter) {
        Ok(_) => println!("PDF created successfully: {:?}", output_path),
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn handle_generate_pdf_from_set_name(set_name: String, output_path: PathBuf) {
    match generate_pdf_from_set_name(&set_name, &output_path, PageSize::Letter) {
        Ok(_) => println!("PDF created successfully: {:?}", output_path),
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn handle_generate_mpc_zip_from_cardlist(cardlist: String, output_path: PathBuf) {
    match generate_mpc_zip_from_cardlist(&cardlist, &output_path) {
        Ok(_) => println!("ZIP created successfully: {:?}", output_path),
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn handle_generate_mpc_zip_from_set_name(set_name: String, output_path: PathBuf) {
    match generate_mpc_zip_from_set_name(&set_name, &output_path) {
        Ok(_) => println!("ZIP created successfully: {:?}", output_path),
        Err(e) => eprintln!("Error: {}", e),
    }
}
