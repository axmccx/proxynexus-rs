use dioxus::prelude::*;
use proxynexus_core::db_storage::DbStorage;
use tracing::error;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");

    #[cfg(feature = "desktop")]
    {
        LaunchBuilder::desktop()
            .with_cfg(
                dioxus::desktop::Config::new()
                    .with_menu(None)
                    .with_window(dioxus::desktop::WindowBuilder::new().with_title("Proxy Nexus")),
            )
            .launch(App);
    }

    #[cfg(feature = "web")]
    {
        launch(App);
    }
}

fn get_db_storage() -> DbStorage {
    #[cfg(target_arch = "wasm32")]
    {
        DbStorage::new_memory()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let home = dirs::home_dir().expect("Could not find home directory");
        let db_path = home.join(".proxynexus").join("proxynexus_data");
        DbStorage::new_sled(&db_path).expect("Failed to initialize sled storage")
    }
}

#[cfg(target_arch = "wasm32")]
async fn hydrate_wasm_db(db: &mut DbStorage) -> Result<(), String> {
    use gloo_net::http::Request;

    let response = Request::get("/init.sql")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch init.sql: {}", e))?;
    
    if !response.ok() {
        return Err(format!("Failed to fetch init.sql: HTTP {}", response.status()));
    }

    let sql = response
        .text()
        .await
        .map_err(|e| format!("Failed to read init.sql text: {}", e))?;
        
    info!("Executing init.sql (size: {} bytes)...", sql.len());
    
    db.execute(&sql)
        .await
        .map_err(|e| format!("Hydration execution error: {}", e))?;
        
    info!("WASM Hydration Complete!");
    Ok(())
}

#[component]
fn App() -> Element {
    let mut db_signal = use_signal(get_db_storage);
    let mut db_ready = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            let mut db = db_signal.write();

            if let Err(e) = db.initialize_schema().await {
                error!("Schema init failed: {}", e);
            }
            
            #[cfg(target_arch = "wasm32")]
            {
                if let Err(e) = hydrate_wasm_db(&mut db).await {
                    error!("WASM Hydration failed: {}", e);
                }
            }
            
            db_ready.set(true);
        });
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        div {
            class: "w-full p-4 bg-gray-800 shadow-md border-b border-gray-700",
            div {
                class: "container mx-auto flex items-center justify-between",
                div {
                    class: "text-xl font-bold text-white hover:text-blue-400 transition-colors",
                    "ProxyNexus"
                }
            }
        }
        
        div {
            class: "p-4",
            if db_ready() {
                div {
                    class: "text-green-500 font-bold",
                    "Database initialized and ready!"
                }
            } else {
                div {
                    class: "text-yellow-500 font-bold animate-pulse",
                    "Loading database..."
                }
            }
        }
    }
}
