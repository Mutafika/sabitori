//! URL → [`ImageData`] fetch + decode for Sabitori.
//!
//! The URL-keyed cache itself now lives in `sabitori-core` so
//! [`ViewContext::image_url`] can access it from view code. This crate
//! provides the HTTP + pixel-decode side:
//!
//! * [`fetch_bytes`] — async HTTP GET returning raw bytes (native: `reqwest`,
//!   wasm: `fetch` via `web-sys`).
//! * [`decode`] — pixel-decode the bytes into an `ImageData` via the `image`
//!   crate.
//!
//! `CacheState` + `ImageCache` are re-exported from `sabitori_core::image_cache`
//! so existing callers don't break.

pub mod decode;
pub mod fetch;

pub use sabitori_core::image_cache::{CacheState, ImageCache};
