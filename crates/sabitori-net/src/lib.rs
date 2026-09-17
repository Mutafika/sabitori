//! URL → [`ImageData`] fetch + decode for Sabitori.
//!
//! The URL-keyed cache itself now lives in `sabitori-core` so
//! [`ViewContext::image_url`] can access it from view code. This crate
//! provides the HTTP + pixel-decode side:
//!
//! * [`http`] — メソッド・ヘッダ・ボディ・Cookie・ステータスを扱える
//!   HTTP クライアント。業務 API を native / wasm 共通で叩くための口 (#63)。
//! * [`fetch_bytes`] — GET してバイト列を返すだけの薄い口 ([`http`] の上に
//!   載っている)。画像ローダが使う。
//! * [`decode`] — pixel-decode the bytes into an `ImageData` via the `image`
//!   crate.
//!
//! `CacheState` + `ImageCache` are re-exported from `sabitori_core::image_cache`
//! so existing callers don't break.

pub mod decode;
pub mod fetch;
pub mod http;

pub use http::{delete, get, patch, post, put, request, Method, NetError, Response};

pub use sabitori_core::image_cache::{CacheState, ImageCache};
