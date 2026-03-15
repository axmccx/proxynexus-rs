use serde::Serialize;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use tracing::{Event, Subscriber};
use tracing_subscriber::{Layer, layer::Context};

const API_KEY: Option<&str> = option_env!("POSTHOG_API_KEY");
pub const CAPTURE_URL: &str = "https://t.proxynexus.net/capture/";

static LOG_BUFFER: Mutex<Vec<String>> = Mutex::new(Vec::new());
static DISTINCT_ID: OnceLock<String> = OnceLock::new();

#[derive(Serialize)]
pub struct GenerationReport {
    pub format: String,
    pub page_size: String,
    pub runtime_ms: u128,
    pub success: bool,
    pub cardlist: String,
    pub error_message: Option<String>,
}

pub struct LogCaptureLayer;

impl<S> Layer<S> for LogCaptureLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();

        if *meta.level() > tracing::Level::INFO || !meta.target().starts_with("proxynexus") {
            return;
        }

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        if let Ok(mut buf) = LOG_BUFFER.lock() {
            buf.push(format!(
                "[{}] {}",
                meta.level().as_str().to_uppercase(),
                visitor.message
            ));
        }
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let s = format!("{:?}", value);
            self.message = s.trim_matches('"').to_string();
        }
    }
}

pub fn start_capture() {
    if let Ok(mut buf) = LOG_BUFFER.lock() {
        buf.clear();
    }
}

pub fn send_report(report: GenerationReport) {
    let Some(key) = API_KEY else { return };

    let logs = if let Ok(mut buf) = LOG_BUFFER.lock() {
        std::mem::take(&mut *buf)
    } else {
        Vec::new()
    };

    let payload = json!({
        "api_key": key,
        "event": "pdf_generated",
        "distinct_id": get_distinct_id(),
        "properties": {
            "app_version": env!("CARGO_PKG_VERSION"),
            "platform": if cfg!(target_arch = "wasm32") { "web" } else { "desktop" },
            "format": report.format,
            "page_size": report.page_size,
            "runtime_ms": report.runtime_ms,
            "success": report.success,
            "cardlist": report.cardlist.lines().collect::<Vec<_>>(),
            "error_message": report.error_message,
            "logs": logs,
        }
    });

    spawn_send(payload);
}

fn get_distinct_id() -> &'static str {
    DISTINCT_ID.get_or_init(|| {
        use uuid::Uuid;

        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::fs;
            if let Some(mut local_dir) = dirs::data_local_dir() {
                local_dir.push("proxynexus");
                let _ = fs::create_dir_all(&local_dir);
                let id_file = local_dir.join("device_id");

                if let Ok(id) = fs::read_to_string(&id_file)
                    && !id.trim().is_empty()
                {
                    return id.trim().to_string();
                }

                let new_id = Uuid::new_v4().to_string();
                let _ = fs::write(&id_file, &new_id);
                return new_id;
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(local_storage)) = window.local_storage() {
                    if let Ok(Some(id)) = local_storage.get_item("proxynexus_device_id") {
                        let id: String = id;
                        if !id.trim().is_empty() {
                            return id;
                        }
                    }

                    let new_id = Uuid::new_v4().to_string();
                    let _ = local_storage.set_item("proxynexus_device_id", &new_id);
                    return new_id;
                }
            }
        }

        Uuid::new_v4().to_string()
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_send(payload: serde_json::Value) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            let client = reqwest::Client::new();
            let _ = client.post(CAPTURE_URL).json(&payload).send().await;
        });
    });
}

#[cfg(target_arch = "wasm32")]
fn spawn_send(payload: serde_json::Value) {
    wasm_bindgen_futures::spawn_local(async move {
        use gloo_net::http::Request;
        if let Ok(req) = Request::post(CAPTURE_URL).json(&payload) {
            let _ = req.send().await;
        }
    });
}

pub fn is_enabled() -> bool {
    API_KEY.is_some()
}
