use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand};
use proxynexus_core::card_source::{CardSource, Cardlist, DecklistUrl, SetName};
use proxynexus_core::catalog::CatalogManager;
use proxynexus_core::collection_builder::build_collection;
use proxynexus_core::collection_manager::CollectionManager;
use proxynexus_core::db_storage::DbStorage;
use proxynexus_core::games::get_card_back_adapter;
use proxynexus_core::image_provider::LocalImageProvider;
use proxynexus_core::models::Printing;
use proxynexus_core::mpc::generate_mpc_zip;
use proxynexus_core::pdf::{
    CutLines, DEFAULT_CUT_LINE_THICKNESS, MAX_CUT_LINE_THICKNESS, MIN_CUT_LINE_THICKNESS, PageSize,
    PdfOptions, generate_pdf,
};
use proxynexus_core::query::{generate_query_output, list_available_sets};
use std::path::PathBuf;
use tracing::info;
use web_time::Instant;

#[derive(Parser)]
#[command(name = "proxynexus-cli")]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, global = true)]
    verbose: bool,

    #[arg(short, long, global = true, default_value = "netrunner")]
    game: String,

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
        .args(["cardlist", "set_name", "decklist_url", "list_sets"]),
    ))]
    Query {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(short, long)]
        decklist_url: Option<String>,

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
            .args(["cardlist", "set_name", "decklist_url"]),
    ))]
    Pdf {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(short, long)]
        decklist_url: Option<String>,

        #[arg(short, long, default_value = "output.pdf")]
        output_path: PathBuf,

        #[arg(long, default_value = "letter")]
        page_size: String,

        #[arg(long, default_value = "margins")]
        cut_lines: Option<String>,

        #[arg(long, default_value_t = DEFAULT_CUT_LINE_THICKNESS)]
        cut_line_thickness: f32,

        #[arg(long, default_value = "edge-to-edge")]
        print_layout: String,

        #[arg(long)]
        upscale: bool,
    },
    #[command(group(
        clap::ArgGroup::new("input")
            .required(true)
            .args(["cardlist", "set_name", "decklist_url"]),
    ))]
    Mpc {
        #[arg(short, long)]
        cardlist: Option<String>,

        #[arg(short, long)]
        set_name: Option<String>,

        #[arg(short, long)]
        decklist_url: Option<String>,

        #[arg(short, long, default_value = "output.zip")]
        output_path: PathBuf,

        #[arg(long)]
        upscale: bool,
    },
    Bleed {
        #[arg(short, long)]
        input_dir: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let home = dirs::home_dir().context("Could not find home directory")?;
    let proxynexus_dir = home.join(".proxynexus");
    let collections_dir = proxynexus_dir.join("collections");

    std::fs::create_dir_all(&collections_dir).with_context(|| {
        format!(
            "Error creating collections directory at {:?}",
            collections_dir
        )
    })?;

    let db_path = proxynexus_dir.join("proxynexus_data");

    let mut db = DbStorage::new_sled(&db_path)
        .with_context(|| format!("Error initializing database at {:?}", db_path))?;

    db.initialize_schema()
        .await
        .context("Error setting up database schema")?;

    let image_provider = LocalImageProvider::new(collections_dir.clone());

    let mut catalog_manager = CatalogManager::new(&mut db);

    if let Err(e) = catalog_manager.seed_if_empty().await {
        eprintln!("Warning: Could not seed catalog: {}", e);
    }

    match cli.command {
        Commands::Collection { action } => {
            handle_collection_action(&mut db, &cli.game, action, collections_dir, cli.verbose).await
        }
        Commands::Generate { output_type } => {
            handle_generate(&mut db, &cli.game, &image_provider, output_type).await
        }
        Commands::Query {
            cardlist,
            set_name,
            decklist_url,
            list_sets,
        } => {
            handle_query(
                &mut db,
                &cli.game,
                cardlist,
                set_name,
                decklist_url,
                list_sets,
            )
            .await
        }
        Commands::Catalog { action } => handle_catalog_action(action, &mut catalog_manager).await,
        Commands::Export { output } => {
            println!("Exporting database to {:?}...", output);
            db.export_sql(&output)
                .await
                .with_context(|| format!("Failed to export database to {:?}", output))?;
            println!("Database exported successfully!");
            Ok(())
        }
    }
}

async fn handle_collection_action(
    db: &mut DbStorage,
    game: &str,
    action: CollectionAction,
    collections_dir: PathBuf,
    verbose: bool,
) -> anyhow::Result<()> {
    match action {
        CollectionAction::Build {
            output,
            images,
            language,
            version,
        } => {
            println!("Writing pnx file...");
            let report = build_collection(game.to_string(), &output, &images, language, version)
                .context("Failed to build collection")?;
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
                .context("Failed to initialize collection manager")?;
            manager
                .add_collection(&path)
                .await
                .with_context(|| format!("Failed to add collection from {:?}", path))?;
            println!("Collection added successfully");
            Ok(())
        }
        CollectionAction::List => {
            let mut manager = CollectionManager::new(db, collections_dir)
                .context("Failed to initialize collection manager")?;
            let collections = manager
                .get_collections()
                .await
                .context("Failed to list collections")?;

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
                .context("Failed to initialize collection manager")?;

            if !manager
                .collection_exists(&name)
                .await
                .context("Failed to check if collection exists")?
            {
                return Err(anyhow!(
                    "Collection '{}' not found. Run 'collection list' to see available collections.",
                    name
                ));
            }

            println!(
                "Are you sure you want to remove collection '{}'? (y/N)",
                name
            );

            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .context("Failed to read user input")?;

            if input.trim().to_lowercase() == "y" {
                manager
                    .remove_collection(&name)
                    .await
                    .with_context(|| format!("Failed to remove collection: {}", name))?;
                println!("Collection '{}' removed successfully.", name);
            }
            Ok(())
        }
    }
}

async fn handle_catalog_action(
    action: CatalogAction,
    catalog_manager: &mut CatalogManager<'_>,
) -> anyhow::Result<()> {
    match action {
        CatalogAction::Update => {
            catalog_manager
                .update_from_api()
                .await
                .context("Failed to update catalog from API")?;
        }
        CatalogAction::Info => {
            println!(
                "{}",
                catalog_manager
                    .get_info()
                    .await
                    .context("Failed to get catalog info")?
            );
        }
    }
    Ok(())
}

enum InputSource {
    Cardlist(String),
    SetName(String),
    DecklistUrl(String),
}

fn determine_input_source(
    cardlist: Option<String>,
    set_name: Option<String>,
    decklist_url: Option<String>,
) -> InputSource {
    if let Some(list) = cardlist {
        InputSource::Cardlist(list)
    } else if let Some(name) = set_name {
        InputSource::SetName(name)
    } else if let Some(url) = decklist_url {
        InputSource::DecklistUrl(url)
    } else {
        unreachable!("clap ensures at least one input is provided")
    }
}

async fn get_printings_from_source(
    db: &mut DbStorage,
    game: &str,
    source: InputSource,
) -> anyhow::Result<Vec<Printing>> {
    let mut store = proxynexus_core::card_store::CardStore::new(db, game.to_string())
        .context("Failed to initialize card store")?;

    let card_requests = match source {
        InputSource::Cardlist(list) => Cardlist(list)
            .to_card_requests(&mut store)
            .await
            .context("Failed to parse cardlist")?,
        InputSource::SetName(name) => SetName(name.clone())
            .to_card_requests(&mut store)
            .await
            .with_context(|| format!("Failed to get cards for set '{}'", name))?,
        InputSource::DecklistUrl(url) => DecklistUrl(url.clone())
            .to_card_requests(&mut store)
            .await
            .with_context(|| format!("Failed to fetch deck from URL: {}", url))?,
    };

    let available = store
        .get_available_printings(&card_requests)
        .await
        .context("Failed to get available printings")?;

    store
        .resolve_printings(&card_requests, &available)
        .context("Failed to resolve printings")
}

async fn handle_generate(
    db: &mut DbStorage,
    game: &str,
    image_provider: &LocalImageProvider,
    output_type: GenerateType,
) -> anyhow::Result<()> {
    match output_type {
        GenerateType::Pdf {
            cardlist,
            set_name,
            decklist_url,
            output_path,
            page_size,
            cut_lines,
            cut_line_thickness,
            print_layout,
            upscale,
        } => {
            let page_size_enum = parse_page_size(&page_size).context("Invalid page size")?;
            let cut_lines_enum =
                parse_cut_lines(cut_lines.as_deref()).context("Invalid cut lines option")?;
            let print_layout_enum =
                parse_print_layout(&print_layout).context("Invalid print layout option")?;
            if !(MIN_CUT_LINE_THICKNESS..=MAX_CUT_LINE_THICKNESS).contains(&cut_line_thickness) {
                return Err(anyhow!(
                    "--cut-line-thickness must be between {} and {} (got {})",
                    MIN_CUT_LINE_THICKNESS,
                    MAX_CUT_LINE_THICKNESS,
                    cut_line_thickness
                ));
            }
            let source = determine_input_source(cardlist, set_name, decklist_url);

            let printings = get_printings_from_source(db, game, source).await?;

            let pdf_bytes = generate_pdf(
                printings,
                image_provider,
                PdfOptions {
                    page_size: page_size_enum,
                    cut_lines: cut_lines_enum,
                    print_layout: print_layout_enum,
                    cut_line_thickness,
                    upscale,
                },
                None,
            )
            .await
            .context("Failed to generate PDF")?;

            std::fs::write(&output_path, pdf_bytes)
                .with_context(|| format!("Failed to write PDF to {:?}", output_path))?;
            println!("PDF created successfully: {:?}", output_path);
            Ok(())
        }

        GenerateType::Mpc {
            cardlist,
            set_name,
            decklist_url,
            output_path,
            upscale,
        } => {
            let source = determine_input_source(cardlist, set_name, decklist_url);
            let start = Instant::now();

            let printings = get_printings_from_source(db, game, source).await?;

            let card_backs = if let Some(adapter) = get_card_back_adapter(game) {
                adapter.fetch_card_backs().await.unwrap_or_default()
            } else {
                vec![]
            };

            let mpc_bytes = generate_mpc_zip(
                printings,
                image_provider,
                proxynexus_core::mpc::MpcOptions { upscale },
                card_backs,
                None,
            )
            .await
            .context("Failed to generate MPC ZIP")?;

            std::fs::write(&output_path, mpc_bytes)
                .with_context(|| format!("Failed to write ZIP to {:?}", output_path))?;
            info!("runtime: {:?}", start.elapsed());
            println!("MPC ZIP created successfully: {:?}", output_path);
            Ok(())
        }
        GenerateType::Bleed { input_dir } => {
            let output_dir = input_dir.join("bleeds");
            std::fs::create_dir_all(&output_dir).with_context(|| {
                format!("Failed to create output directory at {:?}", output_dir)
            })?;
            let mut count = 0;
            for entry in std::fs::read_dir(&input_dir)
                .with_context(|| format!("Failed to read input directory {:?}", input_dir))?
            {
                let entry = entry.context("Failed to read directory entry")?;
                let path = entry.path();
                if path.is_file() {
                    let ext = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if (ext == "png" || ext == "jpg" || ext == "jpeg")
                        && let Ok(img) = image::open(&path)
                    {
                        let bordered = proxynexus_core::print_prep::add_bleed_border(&img);
                        if let Ok(encoded) = proxynexus_core::print_prep::encode_image(
                            bordered,
                            image::ImageFormat::Png,
                        ) {
                            let file_name = path.file_name().unwrap();
                            let out_path = output_dir.join(file_name).with_extension("png");
                            std::fs::write(&out_path, encoded).with_context(|| {
                                format!("Failed to write bleed image to {:?}", out_path)
                            })?;
                            println!("Processed {:?}", path);
                            count += 1;
                        }
                    }
                }
            }
            println!("Bleed generation complete. Processed {} images.", count);
            Ok(())
        }
    }
}

async fn handle_query(
    db: &mut DbStorage,
    game: &str,
    cardlist: Option<String>,
    set_name: Option<String>,
    decklist_url: Option<String>,
    list_sets: bool,
) -> anyhow::Result<()> {
    if list_sets {
        println!("\nAvailable Sets:\n");
        println!(
            "{}",
            list_available_sets(db, game)
                .await
                .context("Failed to list available sets")?
        );
        return Ok(());
    }

    let source = determine_input_source(cardlist, set_name, decklist_url);

    let output = match source {
        InputSource::Cardlist(list) => generate_query_output(&Cardlist(list), db, game).await,
        InputSource::SetName(name) => generate_query_output(&SetName(name), db, game).await,
        InputSource::DecklistUrl(url) => generate_query_output(&DecklistUrl(url), db, game).await,
    };

    println!("\nQuery Results:\n");
    println!("{}", output.context("Query failed")?);

    Ok(())
}

fn parse_page_size(size: &str) -> anyhow::Result<PageSize> {
    match size {
        "letter" => Ok(PageSize::Letter),
        "a4" => Ok(PageSize::A4),
        _ => Err(anyhow!(
            "Unsupported page size: '{}'. Use 'letter' or 'a4'",
            size
        )),
    }
}

fn parse_cut_lines(cut_lines: Option<&str>) -> anyhow::Result<CutLines> {
    match cut_lines {
        Some("none") => Ok(CutLines::None),
        None | Some("margins") => Ok(CutLines::Margins),
        Some("fullpage") => Ok(CutLines::FullPage),
        Some(unsupported) => Err(anyhow!(
            "Unsupported cut lines option: '{}'. Options are 'none', 'margins', or 'fullpage'",
            unsupported
        )),
    }
}

fn parse_print_layout(layout: &str) -> anyhow::Result<proxynexus_core::pdf::PrintLayout> {
    match layout {
        "edge-to-edge" => Ok(proxynexus_core::pdf::PrintLayout::EdgeToEdge),
        "small-margin" => Ok(proxynexus_core::pdf::PrintLayout::SmallMargin),
        "large-margin" => Ok(proxynexus_core::pdf::PrintLayout::LargeMargin),
        "gap" => Ok(proxynexus_core::pdf::PrintLayout::Gap),
        _ => Err(anyhow!(
            "Unsupported print layout: '{}'. Options are 'edge-to-edge', 'small-margin', 'large-margin', 'gap'",
            layout
        )),
    }
}
