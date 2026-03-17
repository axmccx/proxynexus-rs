use clap::{Parser, Subcommand};
use proxynexus_core::card_source::{CardSource, Cardlist, NrdbUrl, SetName};
use proxynexus_core::catalog::Catalog;
use proxynexus_core::collection_builder::build_collection;
use proxynexus_core::collection_manager::CollectionManager;
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::image_provider::LocalImageProvider;
use proxynexus_core::mpc::generate_mpc_zip;
use proxynexus_core::pdf::{PageSize, generate_pdf};
use proxynexus_core::query::{generate_query_output, list_available_sets};
use std::path::PathBuf;
use tracing::info;
use web_time::Instant;

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
    Export {
        #[arg(short, long, default_value = "init.sql")]
        output: PathBuf,
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let home = dirs::home_dir().expect("Could not find home directory");
    let proxynexus_dir = home.join(".proxynexus");
    let collections_dir = proxynexus_dir.join("collections");

    if let Err(e) = std::fs::create_dir_all(&collections_dir) {
        eprintln!("Error creating collections directory: {}", e);
        std::process::exit(1);
    }

    let db_path = proxynexus_dir.join("proxynexus_data");

    let mut db = match DbStorage::new_sled(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error initializing database: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = db.initialize_schema().await {
        eprintln!("Error setting up database schema: {}", e);
        std::process::exit(1);
    }

    let image_provider = LocalImageProvider::new(collections_dir.clone());

    let mut catalog = Catalog::new(&mut db);

    if let Err(e) = catalog.seed_if_empty().await {
        eprintln!("Warning: Could not seed catalog: {}", e);
    }

    let result = match cli.command {
        Commands::Collection { action } => {
            handle_collection_action(action, &mut db, collections_dir, cli.verbose).await
        }
        Commands::Generate { output_type } => {
            handle_generate(&mut db, &image_provider, output_type).await
        }
        Commands::Query {
            cardlist,
            set_name,
            nrdb_url,
            list_sets,
        } => handle_query(&mut db, cardlist, set_name, nrdb_url, list_sets).await,
        Commands::Catalog { action } => handle_catalog_action(action, &mut catalog).await,
        Commands::Export { output } => {
            println!("Exporting database to {:?}...", output);
            match db.export_sql(&output).await {
                Ok(_) => {
                    println!("Database exported successfully!");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn handle_collection_action(
    action: CollectionAction,
    db: &mut DbStorage,
    collections_dir: PathBuf,
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
            let mut manager = CollectionManager::new(db, collections_dir)
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;
            manager
                .add_collection(&path)
                .await
                .map_err(|e| format!("Failed to add collection: {}", e))?;
            println!("Collection added successfully");
            Ok(())
        }
        CollectionAction::List => {
            let mut manager = CollectionManager::new(db, collections_dir)
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;
            let collections = manager
                .get_collections()
                .await
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
            let mut manager = CollectionManager::new(db, collections_dir)
                .map_err(|e| format!("Failed to initialize collection manager: {}", e))?;

            if !manager.collection_exists(&name).await? {
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
                    .await
                    .map_err(|e| format!("Failed to remove collection: {}", e))?;
                println!("Collection '{}' removed successfully.", name);
            }
            Ok(())
        }
    }
}

async fn handle_catalog_action(
    action: CatalogAction,
    catalog: &mut Catalog<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CatalogAction::Update => {
            println!("Fetching latest card data from NetrunnerDB...");
            catalog.update_from_api().await?;
            println!("Card catalog updated successfully!");
        }
        CatalogAction::Info => {
            println!("{}", catalog.get_info().await?);
        }
        CatalogAction::Import { cards, packs } => {
            println!("Loading card data from local files...");
            catalog.update_catalog_from_files(&cards, &packs).await?;
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

async fn handle_generate(
    db: &mut DbStorage,
    image_provider: &LocalImageProvider,
    output_type: GenerateType,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = proxynexus_core::card_store::CardStore::new(db)?;

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

            let printings = match source {
                InputSource::Cardlist(list) => {
                    let card_requests = Cardlist(list).to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&card_requests).await?;
                    store.resolve_printings(&card_requests, &available)?
                }
                InputSource::SetName(name) => {
                    let reqs = SetName(name).to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)?
                }
                InputSource::NrdbUrl(url) => {
                    let reqs = NrdbUrl(url).to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)?
                }
            };

            let pdf_bytes = generate_pdf(printings, image_provider, page_size_enum).await?;

            std::fs::write(&output_path, pdf_bytes)?;
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
            let start = Instant::now();

            let printings = match source {
                InputSource::Cardlist(list) => {
                    let reqs = Cardlist(list).to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)?
                }
                InputSource::SetName(name) => {
                    let reqs = SetName(name).to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)?
                }
                InputSource::NrdbUrl(url) => {
                    let reqs = NrdbUrl(url).to_card_requests(&mut store).await?;
                    let available = store.get_available_printings(&reqs).await?;
                    store.resolve_printings(&reqs, &available)?
                }
            };

            let mpc_bytes = generate_mpc_zip(printings, image_provider).await?;

            std::fs::write(&output_path, mpc_bytes)?;
            info!("runtime: {:?}", start.elapsed());
            println!("MPC ZIP created successfully: {:?}", output_path);
            Ok(())
        }
    }
}

async fn handle_query(
    db: &mut DbStorage,
    cardlist: Option<String>,
    set_name: Option<String>,
    nrdb_url: Option<String>,
    list_sets: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if list_sets {
        println!("\nAvailable Sets:\n");
        println!("{}", list_available_sets(db).await?);
        return Ok(());
    }

    let source = determine_input_source(cardlist, set_name, nrdb_url);

    let output = match source {
        InputSource::Cardlist(list) => generate_query_output(&Cardlist(list), db).await,
        InputSource::SetName(name) => generate_query_output(&SetName(name), db).await,
        InputSource::NrdbUrl(url) => generate_query_output(&NrdbUrl(url), db).await,
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
