/**
 * Runs image upscaling in a web worker.
 *
 * It inits another instance of the main wasm binary in a web worker for the purposes of
 * running the upscale_in_worker Rust function on demand, separate from the main thread.
 */
let wasm_bindgen_module = null;

onmessage = async (e) => {
    if (e.data.type === "init") {
        const url = e.data.url;
        try {
            const module = await import(url);

            let attempts = 0;
            while (!globalThis.__dx_mainWasm && attempts < 500) {
                await new Promise(resolve => setTimeout(resolve, 10));
                attempts++;
            }
            
            if (!globalThis.__dx_mainWasm) {
                throw new Error("Timeout waiting for Dioxus Wasm auto-initialization.");
            }
            
            wasm_bindgen_module = module;
            postMessage({ type: "init_done" });
        } catch (err) {
            postMessage({ type: "error", error: "Failed to initialize worker module: " + err.toString() });
        }
    } else if (e.data.type === "upscale") {
        if (!wasm_bindgen_module) {
            postMessage({ type: "error", id: e.data.id, error: "Worker not initialized" });
            return;
        }
        try {
            const target = (wasm_bindgen_module.upscale_in_worker) ? wasm_bindgen_module : wasm_bindgen_module.default;
                
            if (!target || typeof target.upscale_in_worker !== 'function') {
                throw new Error("upscale_in_worker function not found in Wasm module exports.");
            }
            
            const result = await target.upscale_in_worker(e.data.bytes);
            postMessage(
                { type: "done", id: e.data.id, bytes: result.bytes, width: result.width, height: result.height },
                [result.bytes.buffer]
            );
        } catch (err) {
            postMessage({ type: "error", id: e.data.id, error: "Upscaling failed in worker: " + err.toString() });
        }
    }
};
