//! The windowed GUI self-test.
//!
//! This test *binary* is harness-less (`harness = false` in `Cargo.toml`) so
//! that its `main` runs on the process's main thread — required by `winit` on
//! macOS, which is why the lib unit tests can't drive a real event loop. It
//! runs the GUI once: window and renderer creation, a resize that pads the
//! board, and the `CloseRequested` exit path. It only grows the window, so it
//! works on any real or virtual display.
fn main() {
    rust_life::gui_selftest();
}
