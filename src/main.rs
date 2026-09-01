//! The native entry point: parse the command line, then run the CLI or the
//! GUI for the selected backend.
//!
//! The web build has no command line; its entry point is the
//! `#[wasm_bindgen(start)]` function in the lib (see `rust_life::run_web`), so
//! the bin's `main` is a no-op there and exists only so the bin target still
//! compiles for the wasm target.

#[cfg(not(target_family = "wasm"))]
fn main() {
    rust_life::run()
}

#[cfg(target_family = "wasm")]
fn main() {}
