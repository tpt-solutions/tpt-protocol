# Browser playground

`index.html` decodes a hex frame with TPT Protocol in the browser. Nothing leaves
the page: the decoder is WebAssembly, built from this crate.

## Run it

```bash
# Build the WebAssembly package into ./www/pkg (needs wasm-bindgen 0.2.126, pinned in Cargo.toml)
wasm-bindgen --target web --out-dir www/pkg \
  ../../target/wasm32-unknown-unknown/debug/tpt_protocol.wasm
# (build the .wasm first: cargo build -p tpt-protocol-bindings --features wasm --target wasm32-unknown-unknown)

# Serve the folder (a file:// URL cannot load WebAssembly modules)
python -m http.server 8000 --directory www
```

Then open <http://localhost:8000/>.

`wasm-pack build --target web --out-dir www/pkg -- --features wasm` does the
same in one step.

## Files

- `index.html`: the page. It has no external scripts or stylesheets.
- `pkg/`: generated; not checked in.
