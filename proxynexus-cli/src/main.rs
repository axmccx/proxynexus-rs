use clap::{Parser, Subcommand};
use proxynexus_core::card_source::{Cardlist, NrdbUrl, SetName};
use proxynexus_core::catalog::Catalog;
use proxynexus_core::collection_builder::build_collection;
use proxynexus_core::collection_manager::CollectionManager;
use proxynexus_core::mpc::generate_mpc_zip;
use proxynexus_core::pdf::{PageSize, generate_pdf};
use proxynexus_core::query::{generate_query_output, list_available_sets};
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
    Catalog {
        #[command(subcommand)]
        action: CatalogAction,
    },
    Collection {
        #[command(subcommand)]
        action: CollectionAction,
    },
    #[command(group(
    clap::ArgGroup::new("input")
        .required(true)
        .args(["cardlist", "set_name", "nrdb_url", "list_sets"]),
    ))]
    Query {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(long)]
        nrdb_url: Option<String>,

        #[arg(long)]
        list_sets: bool,
    },
}

#[derive(Subcommand)]
enum CatalogAction {
    Update,
    Info,
    Import {
        #[arg(short, long)]
        cards: PathBuf,

        #[arg(short, long)]
        packs: PathBuf,
    },
}

#[derive(Subcommand)]
enum CollectionAction {
    Build {
        #[arg(short, long)]
        images: PathBuf,

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

    let catalog = match Catalog::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error initializing catalog: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = catalog.seed_if_empty() {
        eprintln!("Warning: Could not seed catalog: {}", e);
    }

    let result = match cli.command {
        Commands::Collection { action } => handle_collection_action(action, cli.verbose),
        Commands::Generate { output_type } => handle_generate(output_type),
        Commands::Query {
            cardlist,
            set_name,
            nrdb_url,
            list_sets,
        } => handle_query(cardlist, set_name, nrdb_url, list_sets),
        Commands::Catalog { action } => handle_catalog_action(action, &catalog),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn handle_collection_action(
    action: CollectionAction,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CollectionAction::Build {
            output,
            images,
            language,
            version,
        } => {
            println!("Writing pnx file...");
            let report = build_collection(&output, &images, language, version)?;
            println!("Added {} printings", report.printings_added);
            println!("Collection created: {:?}", output);
            if verbose {
                for path in &report.image_paths {
                    println!("  {}", path.file_name().unwrap().to_string_lossy());
                }
            }
            Ok(())
        }
        CollectionAction::Add { path } => {
            let manager = CollectionManager::new()
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;
            manager
                .add_collection(&path)
                .map_err(|e| format!("Failed to add collection: {}", e))?;
            println!("Collection added successfully");
            Ok(())
        }
        CollectionAction::List => {
            let manager = CollectionManager::new()
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;
            let collections = manager
                .get_collections()
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
        }
        CollectionAction::Remove { name } => {
            let manager = CollectionManager::new()
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;

            if !manager.collection_exists(&name)? {
                return Err(format!("Collection '{}' not found. Run 'collection list' to see available collections.", name).into());
            }

            println!(
                "Are you sure you want to remove collection '{}'? (y/N)",
                name
            );

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if input.trim().to_lowercase() == "y" {
                manager
                    .remove_collection(&name)
                    .map_err(|e| format!("Failed to remove collection: {}", e))?;
                println!("Collection '{}' removed successfully.", name);
            }
            Ok(())
        }
    }
}

fn handle_catalog_action(
    action: CatalogAction,
    catalog: &Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CatalogAction::Update => {
            println!("Fetching latest card data from NetrunnerDB...");
            catalog.update_from_api()?;
            println!("Card catalog updated successfully!");
        }
        CatalogAction::Info => {
            println!("{}", catalog.get_info()?);
        }
        CatalogAction::Import { cards, packs } => {
            println!("Loading card data from local files...");
            catalog.update_catalog_from_files(&cards, &packs)?;
            println!("Card catalog updated successfully!");
        }
    }
    Ok(())
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
                InputSource::Cardlist(list) => {
                    generate_pdf(&Cardlist(list), &output_path, page_size_enum)?
                }
                InputSource::SetName(name) => {
                    generate_pdf(&SetName(name), &output_path, page_size_enum)?
                }
                InputSource::NrdbUrl(url) => {
                    generate_pdf(&NrdbUrl(url), &output_path, page_size_enum)?
                }
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

            let start = std::time::Instant::now();

            match source {
                InputSource::Cardlist(list) => generate_mpc_zip(&Cardlist(list), &output_path)?,
                InputSource::SetName(name) => generate_mpc_zip(&SetName(name), &output_path)?,
                InputSource::NrdbUrl(url) => generate_mpc_zip(&NrdbUrl(url), &output_path)?,
            }

            eprintln!("runtime: {:?}", start.elapsed());
            println!("MPC ZIP created successfully: {:?}", output_path);
            Ok(())
        }
    }
}

fn handle_query(
    cardlist: Option<String>,
    set_name: Option<String>,
    nrdb_url: Option<String>,
    list_sets: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if list_sets {
        println!("\nAvailable Sets:\n");
        println!("{}", list_available_sets()?);
        return Ok(());
    }

    let source = determine_input_source(cardlist, set_name, nrdb_url);

    let output = match source {
        InputSource::Cardlist(list) => generate_query_output(&Cardlist(list)),
        InputSource::SetName(name) => generate_query_output(&SetName(name)),
        InputSource::NrdbUrl(url) => generate_query_output(&NrdbUrl(url)),
    };

    println!("\nQuery Results:\n");
    println!("{}", output?);

    Ok(())
}

fn parse_page_size(size: &str) -> Result<PageSize, String> {
    match size {
        "letter" => Ok(PageSize::Letter),
        "a4" => Ok(PageSize::A4),
        _ => Err(format!(
            "Unsupported page size: '{}'. Use 'letter' or 'a4'",
            size
        )),
    }
}
