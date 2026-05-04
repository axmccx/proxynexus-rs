use crate::error::{ProxyNexusError, Result};
use async_once_cell::OnceCell;
use burn::tensor::Tensor;
use burn::tensor::backend::Backend;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

const SCALE: usize = 4; // needs to match the scale value used when creating the onnx file
const TILE_SIZE: usize = 128;
const TILE_PAD: usize = 5;

type WgpuBackend = burn_wgpu::Wgpu<f32, i32>;

#[cfg(target_arch = "wasm32")]
type PendingRequestsMap =
    Rc<RefCell<HashMap<String, futures::channel::oneshot::Sender<Result<Vec<u8>>>>>>;

#[derive(Debug)]
struct InferenceState {
    model: model::Model<WgpuBackend>,
    device: <WgpuBackend as Backend>::Device,
}

static STATE: OnceCell<InferenceState> = OnceCell::new();

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WORKER_STATE: RefCell<Option<WorkerState>> = const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
static REQUEST_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[cfg(target_arch = "wasm32")]
struct WorkerState {
    worker: web_sys::Worker,
    pending_requests: PendingRequestsMap,
    _onmessage: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
    _onerror: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

pub mod model {
    include!(concat!(env!("OUT_DIR"), "/realesr-general-x4v3.rs"));
}

#[cfg(target_arch = "wasm32")]
pub async fn probe_webgpu() -> bool {
    use js_sys::{Function, Promise, Reflect};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let available = async {
        let window = web_sys::window()?;
        let nav = window.navigator();
        let gpu = Reflect::get(&nav, &"gpu".into()).ok()?;
        if gpu.is_undefined() || gpu.is_null() {
            return None;
        }
        let request_adapter_fn: Function = Reflect::get(&gpu, &"requestAdapter".into())
            .ok()?
            .dyn_into()
            .ok()?;
        let adapter_promise: Promise = request_adapter_fn.call0(&gpu).ok()?.dyn_into().ok()?;
        let adapter = JsFuture::from(adapter_promise).await.ok()?;
        if adapter.is_null() || adapter.is_undefined() {
            return None;
        }
        let request_device_fn: Function = Reflect::get(&adapter, &"requestDevice".into())
            .ok()?
            .dyn_into()
            .ok()?;
        let device_promise: Promise = request_device_fn.call0(&adapter).ok()?.dyn_into().ok()?;
        let device = JsFuture::from(device_promise).await.ok()?;
        if device.is_null() || device.is_undefined() {
            return None;
        }
        Some(())
    }
    .await;

    available.is_some()
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn upscale_image(bytes: &[u8]) -> Result<Vec<u8>> {
    upscale_image_inner(bytes).await
}

#[cfg(target_arch = "wasm32")]
pub async fn upscale_image(bytes: &[u8]) -> Result<Vec<u8>> {
    use js_sys::{Object, Reflect, Uint8Array};

    let (worker, pending_requests) = get_or_init_worker().await?;

    let id = REQUEST_COUNTER
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .to_string();
    let (response_tx, response_rx) = futures::channel::oneshot::channel();

    pending_requests
        .borrow_mut()
        .insert(id.clone(), response_tx);

    let js_request = Object::new();
    Reflect::set(&js_request, &"type".into(), &"upscale".into()).unwrap();
    Reflect::set(&js_request, &"id".into(), &id.clone().into()).unwrap();

    let bytes_vec = bytes.to_vec();
    let js_image_array = Uint8Array::from(bytes_vec.as_slice());
    let js_image_buffer = js_image_array.buffer();
    Reflect::set(&js_request, &"bytes".into(), &js_image_array.into()).unwrap();

    let buffers_to_transfer = js_sys::Array::of1(&js_image_buffer);

    worker
        .post_message_with_transfer(&js_request, &buffers_to_transfer)
        .map_err(move |e| {
            pending_requests.borrow_mut().remove(&id);
            ProxyNexusError::Internal(format!("Failed to post upscale message: {:?}", e))
        })?;

    let result = response_rx.await.map_err(|_| {
        ProxyNexusError::Internal("Worker channel closed unexpectedly".to_string())
    })??;
    Ok(result)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn upscale_in_worker(
    bytes: js_sys::Uint8Array,
) -> std::result::Result<Vec<u8>, wasm_bindgen::JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let raw_bytes = bytes.to_vec();
    let result = upscale_image_inner(&raw_bytes)
        .await
        .map_err(|e| wasm_bindgen::JsValue::from_str(&e.to_string()))?;
    Ok(result)
}

#[cfg(target_arch = "wasm32")]
async fn get_or_init_worker() -> Result<(web_sys::Worker, PendingRequestsMap)> {
    use js_sys::{Object, Reflect};
    use wasm_bindgen::prelude::*;
    use web_sys::{Event, MessageEvent, Worker};

    if let Some((worker, requests)) = WORKER_STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .map(|s| (s.worker.clone(), s.pending_requests.clone()))
    }) {
        return Ok((worker, requests));
    }

    let options = web_sys::WorkerOptions::new();
    options.set_type(web_sys::WorkerType::Module);
    let worker = Worker::new_with_options("/worker/upscaler_worker.js", &options)
        .map_err(|e| ProxyNexusError::Internal(format!("Failed to create worker: {:?}", e)))?;

    let pending_requests: PendingRequestsMap = Rc::new(RefCell::new(HashMap::new()));
    let pending_requests_msg = pending_requests.clone();
    let pending_requests_err = pending_requests.clone();

    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        let data = e.data();
        let msg_type = Reflect::get(&data, &"type".into()).unwrap_or(JsValue::NULL);

        if msg_type == "done" {
            let id_val = Reflect::get(&data, &"id".into()).unwrap();
            let id = id_val.as_string().unwrap_or_default();

            if let Some(sender) = pending_requests_msg.borrow_mut().remove(&id) {
                let bytes_val = Reflect::get(&data, &"bytes".into()).unwrap();
                let array = js_sys::Uint8Array::unchecked_from_js(bytes_val);
                let _ = sender.send(Ok(array.to_vec()));
            }
        } else if msg_type == "error" {
            let id_val = Reflect::get(&data, &"id".into()).unwrap_or(JsValue::NULL);
            if let Some(id) = id_val.as_string()
                && let Some(sender) = pending_requests_msg.borrow_mut().remove(&id)
            {
                let err_val = Reflect::get(&data, &"error".into()).unwrap();
                let err_str = err_val
                    .as_string()
                    .unwrap_or_else(|| "Unknown error".to_string());
                let _ = sender.send(Err(ProxyNexusError::Internal(err_str)));
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    let onerror = Closure::wrap(Box::new(move |_e: Event| {
        let mut pending = pending_requests_err.borrow_mut();
        for (_, sender) in pending.drain() {
            let _ = sender.send(Err(ProxyNexusError::Internal(
                "Worker error occurred".to_string(),
            )));
        }

        WORKER_STATE.with(|state| {
            *state.borrow_mut() = None;
        });
    }) as Box<dyn FnMut(Event)>);

    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    worker.set_onerror(Some(onerror.as_ref().unchecked_ref()));

    // Wait for worker to finish initializing
    let (init_tx, init_rx) = futures::channel::oneshot::channel();
    let init_tx_rc = Rc::new(RefCell::new(Some(init_tx)));

    let oninit = Closure::wrap(Box::new(move |e: MessageEvent| {
        let data = e.data();
        let msg_type = Reflect::get(&data, &"type".into()).unwrap_or(JsValue::NULL);
        if msg_type == "init_done" {
            if let Some(tx) = init_tx_rc.borrow_mut().take() {
                let _ = tx.send(Ok(()));
            }
        } else if msg_type == "error"
            && let Some(tx) = init_tx_rc.borrow_mut().take()
        {
            let err_val = Reflect::get(&data, &"error".into()).unwrap();
            let _ = tx.send(Err(err_val.as_string().unwrap_or_default()));
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    worker.set_onmessage(Some(oninit.as_ref().unchecked_ref()));

    // janky search to find the gui js with the dioxus hash
    let mut js_url = "/assets/proxynexus-gui.js".to_string();
    if let Some(window) = web_sys::window()
        && let Some(document) = window.document()
        && let Ok(scripts) = Reflect::get(&document, &"scripts".into())
        && let Ok(length_val) = Reflect::get(&scripts, &"length".into())
        && let Some(length) = length_val.as_f64()
    {
        for i in 0..(length as u32) {
            if let Ok(script) = Reflect::get(&scripts, &i.into())
                && let Ok(src_val) = Reflect::get(&script, &"src".into())
                && let Some(src) = src_val.as_string()
                && src.contains("proxynexus-gui")
                && src.ends_with(".js")
            {
                js_url = src;
                break;
            }
        }
    }

    let init_msg = Object::new();
    Reflect::set(&init_msg, &"type".into(), &"init".into()).unwrap();
    Reflect::set(&init_msg, &"url".into(), &js_url.into()).unwrap();
    worker.post_message(&init_msg).map_err(|e| {
        ProxyNexusError::Internal(format!("Failed to send init message to worker: {:?}", e))
    })?;

    match init_rx.await {
        Ok(Ok(_)) => {
            worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            drop(oninit);
        }
        Ok(Err(e)) => {
            return Err(ProxyNexusError::Internal(format!(
                "Worker failed to initialize: {}",
                e
            )));
        }
        Err(_) => {
            return Err(ProxyNexusError::Internal(
                "Worker died during initialization".to_string(),
            ));
        }
    }

    WORKER_STATE.with(|state| {
        *state.borrow_mut() = Some(WorkerState {
            worker: worker.clone(),
            pending_requests: pending_requests.clone(),
            _onmessage: onmessage,
            _onerror: onerror,
        });
    });

    Ok((worker, pending_requests))
}

async fn upscale_image_inner(bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(bytes)?;
    let img_rgb = img.to_rgb8();

    let state = STATE
        .get_or_try_init(async {
            let device = burn_wgpu::WgpuDevice::default();

            #[cfg(target_arch = "wasm32")]
            {
                std::panic::set_hook(Box::new(console_error_panic_hook::hook));
                burn_wgpu::init_setup_async::<burn_wgpu::graphics::WebGpu>(
                    &device,
                    Default::default(),
                )
                .await;
            }

            let weights = fetch_model_weights().await?;
            let model = model::Model::<WgpuBackend>::from_bytes(
                burn::tensor::Bytes::from_bytes_vec(weights),
                &device,
            );

            Ok::<InferenceState, ProxyNexusError>(InferenceState { model, device })
        })
        .await?;

    let out_img = process_tiled(&img_rgb, &state.model, &state.device).await?;
    finalize_upscale_img(out_img)
}

async fn process_tiled<B: Backend>(
    img_rgb: &image::RgbImage,
    model: &model::Model<B>,
    device: &B::Device,
) -> Result<image::RgbImage> {
    let width = img_rgb.width() as usize;
    let height = img_rgb.height() as usize;
    let out_width = width * SCALE;
    let out_height = height * SCALE;
    let mut out_img = image::RgbImage::new(out_width as u32, out_height as u32);

    for in_y in (0..height).step_by(TILE_SIZE) {
        for in_x in (0..width).step_by(TILE_SIZE) {
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::future::sleep(std::time::Duration::from_millis(1)).await;
            }

            let in_w = (TILE_SIZE).min(width - in_x);
            let in_h = (TILE_SIZE).min(height - in_y);

            let pad_left = (in_x).min(TILE_PAD);
            let pad_top = (in_y).min(TILE_PAD);
            let pad_right = (width - (in_x + in_w)).min(TILE_PAD);
            let pad_bottom = (height - (in_y + in_h)).min(TILE_PAD);

            let tile_x = in_x - pad_left;
            let tile_y = in_y - pad_top;
            let tile_w = in_w + pad_left + pad_right;
            let tile_h = in_h + pad_top + pad_bottom;

            let mut planar_pixels = vec![0.0; 3 * tile_w * tile_h];
            let area = tile_w * tile_h;

            for ty in 0..tile_h {
                for tx in 0..tile_w {
                    let pixel = img_rgb.get_pixel((tile_x + tx) as u32, (tile_y + ty) as u32);
                    let i = ty * tile_w + tx;
                    planar_pixels[i] = pixel[0] as f32 / 255.0;
                    planar_pixels[area + i] = pixel[1] as f32 / 255.0;
                    planar_pixels[area * 2 + i] = pixel[2] as f32 / 255.0;
                }
            }

            let out_data = run_inference(model, device, planar_pixels, tile_w, tile_h).await?;

            let out_tile_w = tile_w * SCALE;
            let out_area = out_tile_w * tile_h * SCALE;

            let out_pad_left = pad_left * SCALE;
            let out_pad_top = pad_top * SCALE;
            let out_in_w = in_w * SCALE;
            let out_in_h = in_h * SCALE;

            for ty in 0..out_in_h {
                for tx in 0..out_in_w {
                    let src_x = out_pad_left + tx;
                    let src_y = out_pad_top + ty;
                    let src_i = src_y * out_tile_w + src_x;

                    let r = (out_data[src_i].clamp(0.0, 1.0) * 255.0) as u8;
                    let g = (out_data[out_area + src_i].clamp(0.0, 1.0) * 255.0) as u8;
                    let b = (out_data[out_area * 2 + src_i].clamp(0.0, 1.0) * 255.0) as u8;

                    out_img.put_pixel(
                        (in_x * SCALE + tx) as u32,
                        (in_y * SCALE + ty) as u32,
                        image::Rgb([r, g, b]),
                    );
                }
            }
        }
    }

    Ok(out_img)
}

async fn run_inference<B: Backend>(
    model: &model::Model<B>,
    device: &B::Device,
    planar_pixels: Vec<f32>,
    tile_w: usize,
    tile_h: usize,
) -> Result<Vec<f32>> {
    let tensor_data = burn::tensor::TensorData::new(planar_pixels, vec![1, 3, tile_h, tile_w]);
    let input_tensor: Tensor<B, 4> = Tensor::from_data(tensor_data, device);
    let output_tensor = model.forward(input_tensor);
    let out_data = output_tensor
        .into_data_async()
        .await
        .map_err(|e| ProxyNexusError::Internal(format!("Inference failed: {}", e)))?
        .to_vec::<f32>()
        .map_err(|e| ProxyNexusError::Internal(format!("Data conversion failed: {}", e)))?;
    Ok(out_data)
}

fn finalize_upscale_img(out_img: image::RgbImage) -> Result<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 95);

    out_img
        .write_with_encoder(encoder)
        .map_err(|e| ProxyNexusError::Internal(e.to_string()))?;

    Ok(cursor.into_inner())
}

async fn fetch_model_weights() -> Result<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    {
        let response = gloo_net::http::Request::get("/realesr-general-x4v3.bpk")
            .send()
            .await
            .map_err(|e| ProxyNexusError::Internal(format!("Fetch failed: {}", e)))?;

        if !response.ok() {
            return Err(ProxyNexusError::Internal(format!(
                "Failed to fetch model weights for upscaler: HTTP {} {}",
                response.status(),
                response.status_text()
            )));
        }

        let bytes = response
            .binary()
            .await
            .map_err(|e| ProxyNexusError::Internal(format!("Buffer failed: {}", e)))?;
        Ok(bytes)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(include_bytes!(concat!(env!("OUT_DIR"), "/realesr-general-x4v3.bpk")).to_vec())
    }
}
