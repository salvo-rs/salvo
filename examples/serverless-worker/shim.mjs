// Cloudflare Workers module-worker shim.
//
// Instantiates the wasm-bindgen module (generated into `build/`) and exposes its
// `fetch` handler as the default module-worker export. The wasm is imported
// directly so Wrangler hands it to `initSync` as a `WebAssembly.Module`.
//
// Regenerate `build/` before running:
//   cargo build --release --target wasm32-unknown-unknown
//   wasm-bindgen --target web --out-dir build \
//     ../target/wasm32-unknown-unknown/release/example_serverless_worker.wasm
import wasmModule from "./build/example_serverless_worker_bg.wasm";
import { initSync, fetch } from "./build/example_serverless_worker.js";

initSync({ module: wasmModule });

export default {
  async fetch(request, env, ctx) {
    return fetch(request, env, ctx);
  },
};