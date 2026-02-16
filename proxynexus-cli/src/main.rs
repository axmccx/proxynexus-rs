use clap::{Parser, Subcommand};
use proxynexus_core::collection_builder::CollectionBuilder;
use proxynexus_core::collection_manager::CollectionManager;
use proxynexus_core::mpc;
use proxynexus_core::pdf;
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
            .args(["cardlist", "set_name", "nrdb_url"]),
    ))]
    Pdf {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(long)]
        nrdb_url: Option<String>,

        #[arg(short, long, default_value = "output.pdf")]
        output_path: PathBuf,

        #[arg(long, default_value = "letter")]
        page_size: String,
    },

    #[command(group(
        clap::ArgGroup::new("input")
            .required(true)
            .args(["cardlist", "set_name", "nrdb_url"]),
    ))]
    Mpc {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(long)]
        nrdb_url: Option<String>,

        #[arg(short, long, default_value = "output.zip")]
        output_path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Collection { action } => handle_collection_action(action, cli.verbose),
        Commands::Generate { output_type } => handle_generate(output_type),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn handle_collection_action(action: CollectionAction, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CollectionAction::Build {
            output,
            images,
            metadata,
            language,
            version
        } => {
            CollectionBuilder::new(output, images, metadata, language, version)
                .verbose(verbose)
                .build()
                .map_err(|e| format!("Build failed: {}", e))?;
            Ok(())
        }
        CollectionAction::Add { path } => {
            let manager = CollectionManager::new()
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;
            manager.add_collection(&path)
                .map_err(|e| format!("Failed to add collection: {}", e))?;
            println!("Collection added successfully");
            Ok(())
        },
        CollectionAction::List => {
            let manager = CollectionManager::new()
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;
            let collections = manager.get_collections()
                .map_err(|e| format!("Failed to list collections: {}", e))?;

            if collections.is_empty() {
                println!("No collections available. Use 'collection add <file.pnx>' to add one.");
            } else {
                println!("Available collections:");
                for (name, version, language) in &collections {
                    println!("  {} (v{}, {})", name, version, language);
                }
            }
            Ok(())
        },
        CollectionAction::Remove { name } => {
            println!("Are you sure you want to remove collection '{}'? (y/N)", name);

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if input.trim().to_lowercase() == "y" {
                let manager = CollectionManager::new()
                    .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;
                manager.remove_collection(&name)
                    .map_err(|e| format!("Failed to remove collection: {}", e))?;
                println!("Collection '{}' removed successfully.", name);
            }
            Ok(())
        },
    }
}

enum InputSource {
    NrdbUrl(String),
    Cardlist(String),
    SetName(String),
}

fn determine_input_source(
    cardlist: Option<String>,
    set_name: Option<String>,
    nrdb_url: Option<String>,
) -> InputSource {
    if let Some(list) = cardlist {
        InputSource::Cardlist(list)
    } else if let Some(name) = set_name {
        InputSource::SetName(name)
    } else if let Some(url) = nrdb_url {
        InputSource::NrdbUrl(url)
    } else {
        unreachable!("clap ensures at least one input is provided")
    }
}

fn handle_generate(output_type: GenerateType) -> Result<(), Box<dyn std::error::Error>> {
    match output_type {
        GenerateType::Pdf {
            cardlist,
            set_name,
            nrdb_url,
            output_path,
            page_size,
        } => {
            let page_size_enum = parse_page_size(&page_size)?;
            let source = determine_input_source(cardlist, set_name, nrdb_url);

            match source {
                InputSource::Cardlist(list) =>
                    pdf::generate_pdf_from_cardlist(&list, &output_path, page_size_enum)?,
                InputSource::SetName(name) =>
                    pdf::generate_pdf_from_set_name(&name, &output_path, page_size_enum)?,
                InputSource::NrdbUrl(url) =>
                    pdf::generate_pdf_from_nrdb_url(&url, &output_path, page_size_enum)?,
            }

            println!("PDF created successfully: {:?}", output_path);
            Ok(())
        }

        GenerateType::Mpc {
            cardlist,
            set_name,
            nrdb_url,
            output_path,
        } => {
            let source = determine_input_source(cardlist, set_name, nrdb_url);

            match source {
                InputSource::Cardlist(list) =>
                    mpc::generate_mpc_zip_from_cardlist(&list, &output_path)?,
                InputSource::SetName(name) =>
                    mpc::generate_mpc_zip_from_set_name(&name, &output_path)?,
                InputSource::NrdbUrl(url) =>
                    mpc::generate_mpc_zip_from_nrdb_url(&url, &output_path)?,
            }

            println!("MPC ZIP created successfully: {:?}", output_path);
            Ok(())
        }
    }
}

fn parse_page_size(size: &str) -> Result<pdf::PageSize, String> {
    match size {
        "letter" => Ok(pdf::PageSize::Letter),
        "a4" => Ok(pdf::PageSize::A4),
        _ => Err(format!("Unsupported page size: '{}'. Use 'letter' or 'a4'", size)),
    }
}
