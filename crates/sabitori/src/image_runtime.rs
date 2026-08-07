//! Glue that turns `ViewContext::image_url` calls into background fetches
//! and drains results back into the shared cache each frame.
//!
//! The runtime holds:
//! * `image_cache` — the authoritative `sabitori_core::image_cache::ImageCache`
//!   the view reads from.
//! * `image_pending` — results queued by background tasks, applied to the
//!   cache at the top of every frame.
//! * `runtime_handle` (native only) — a dedicated tokio multi-thread runtime
//!   so fetch + decode don't block the UI thread.
//!
//! Constructing the `ImageCtx` here keeps the ugly platform split isolated.

use std::sync::{Arc, Mutex};

use sabitori_core::image_cache::{CacheState, ImageCache};
use sabitori_core::ImageCtx;

/// Results from background fetches, keyed by the request URL.
pub type PendingQueue = Arc<Mutex<Vec<(String, CacheState)>>>;

/// Drop all finished fetches into the shared cache. Call once per frame
/// before building the view.
pub fn drain_pending(
    cache: &Arc<Mutex<ImageCache>>,
    pending: &PendingQueue,
) {
    let drained: Vec<_> = {
        let mut p = pending.lock().unwrap();
        if p.is_empty() { return; }
        p.drain(..).collect()
    };
    let mut c = cache.lock().unwrap();
    for (url, state) in drained {
        c.insert(&url, state);
    }
}

/// Build an `ImageCtx` whose `request` closure spawns `fetch_bytes` +
/// `decode_image` in the background, writing the result into `pending`.
/// Already-queued URLs are skipped via the cache's `Loading` marker.
#[cfg(not(target_arch = "wasm32"))]
pub fn make_image_ctx(
    cache: Arc<Mutex<ImageCache>>,
    pending: PendingQueue,
    rt: tokio::runtime::Handle,
) -> ImageCtx {
    let cache_for_closure = cache.clone();
    let pending_for_closure = pending.clone();
    let request: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |url: &str| {
        // Mark Loading immediately so repeated `image_url` calls in the
        // same frame (or in follow-up frames before fetch completes) don't
        // re-spawn.
        {
            let mut c = cache_for_closure.lock().unwrap();
            if !matches!(c.get(url), CacheState::Missing) {
                return;
            }
            c.mark_loading(url);
        }
        let url_owned = url.to_string();
        let pending = pending_for_closure.clone();
        rt.spawn(async move {
            let result = match sabitori_net::fetch::fetch_bytes(&url_owned).await {
                Ok(bytes) => match sabitori_net::decode::decode_image(&bytes) {
                    Ok(data) => CacheState::Loaded(data),
                    Err(e) => CacheState::Failed(e),
                },
                Err(e) => CacheState::Failed(e),
            };
            pending.lock().unwrap().push((url_owned, result));
        });
    });
    ImageCtx { cache, request }
}

/// WASM variant: `spawn_local` instead of a tokio runtime.
#[cfg(target_arch = "wasm32")]
pub fn make_image_ctx(
    cache: Arc<Mutex<ImageCache>>,
    pending: PendingQueue,
) -> ImageCtx {
    let cache_for_closure = cache.clone();
    let pending_for_closure = pending.clone();
    let request: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |url: &str| {
        {
            let mut c = cache_for_closure.lock().unwrap();
            if !matches!(c.get(url), CacheState::Missing) {
                return;
            }
            c.mark_loading(url);
        }
        let url_owned = url.to_string();
        let pending = pending_for_closure.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = match sabitori_net::fetch::fetch_bytes(&url_owned).await {
                Ok(bytes) => match sabitori_net::decode::decode_image(&bytes) {
                    Ok(data) => CacheState::Loaded(data),
                    Err(e) => CacheState::Failed(e),
                },
                Err(e) => CacheState::Failed(e),
            };
            pending.lock().unwrap().push((url_owned, result));
        });
    });
    ImageCtx { cache, request }
}
