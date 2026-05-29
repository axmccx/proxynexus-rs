use crate::export::ExportOptions;
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
    pub options: ExportOptions,
    pub runtime_ms: u128,
    pub success: bool,
    pub active_game_id: String,
    pub source_type: &'static str,
    pub source_text: String,
    pub selected_printings: Vec<String>,
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

    #[cfg(target_arch = "wasm32")]
    let (os, browser) = {
        let mut os = "unknown".to_string();
        let mut browser = "unknown".to_string();
        if let Some(window) = web_sys::window() {
            if let Ok(user_agent) = window.navigator().user_agent() {
                browser = user_agent.clone();
                let ua_lower = user_agent.to_lowercase();
                if ua_lower.contains("windows") {
                    os = "windows".to_string();
                } else if ua_lower.contains("iphone")
                    || ua_lower.contains("ipad")
                    || ua_lower.contains("ios")
                {
                    os = "ios".to_string();
                } else if ua_lower.contains("mac") {
                    if window.navigator().max_touch_points() > 1 {
                        os = "ios".to_string();
                    } else {
                        os = "macos".to_string();
                    }
                } else if ua_lower.contains("android") {
                    os = "android".to_string();
                } else if ua_lower.contains("linux") {
                    os = "linux".to_string();
                }
            }
        }
        (os, browser)
    };

    #[cfg(not(target_arch = "wasm32"))]
    let (os, browser) = { (std::env::consts::OS.to_string(), "desktop_app".to_string()) };

    let payload = json!({
        "api_key": key,
        "event": "export_generated",
        "distinct_id": get_distinct_id(),
        "properties": {
            "app_version": env!("CARGO_PKG_VERSION"),
            "platform": if cfg!(target_arch = "wasm32") { "web" } else { "desktop" },
            "os": os,
            "browser": browser,
            "game_id": report.active_game_id,
            "format": report.format,
            "options": report.options,
            "runtime_ms": report.runtime_ms,
            "success": report.success,
            "source_type": report.source_type,
            "source_text": report.source_text.lines().collect::<Vec<_>>(),
            "selected_printings": report.selected_printings,
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
    if let Some(window) = web_sys::window() {
        if let Ok(body_str) = serde_json::to_string(&payload) {
            let _ = window
                .navigator()
                .send_beacon_with_opt_str(CAPTURE_URL, Some(&body_str));
        }
    }
}

pub fn is_enabled() -> bool {
    API_KEY.is_some()
}
