//! DeckCraft in the browser.
//!
//! Runs the same [`deckcraft_ui_egui::SlideApp`] as the desktop app through eframe's web
//! runner (wgpu: WebGPU where available, WebGL2 otherwise). Build with `trunk build --release`
//! from this directory (output in `dist/web`).
//!
//! Differences from the desktop app:
//! - no TCP control channel (browsers can't listen on sockets);
//! - File → Open and Insert → Pictures/Audio/Video use the browser file picker; bytes arrive
//!   asynchronously through `Services::inbox` (presentations open, media is inserted);
//! - Save and Export trigger a browser download;
//! - dropped files are read asynchronously by `web::WebShell` and delivered through the inbox;
//! - preferences are not persisted.
//!
//! URL query flags: `?webgl` forces the WebGL2 backend instead of WebGPU; `?blank` starts with an
//! empty presentation instead of the sample deck; `?show` starts the slide show.
//!
//! Self-hosted deployments can build with `--features self-hosted-update` (off by default): a
//! notice tells visitors when a newer release exists and the administrator can start the update
//! (`updates`, docs/self-hosted-updates.md).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

// The web shell uses it; natively it only exists for its tests.
#[cfg(all(feature = "self-hosted-update", any(target_arch = "wasm32", test)))]
mod updates;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("deckcraft-web only runs in the browser: build it with `trunk build --release` in apps/deckcraft-web");
}
