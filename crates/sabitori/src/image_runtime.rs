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

use sabitori_core::image_cache::{CacheState, ImageCache, ImageRequest};
use sabitori_core::ImageCtx;

/// Results from background fetches, keyed by [`ImageRequest::key`].
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

/// 取って、読んで (縮める指定があれば縮めて)、キャッシュに置く形にする。
async fn load(url: &str, max_px: Option<u32>) -> CacheState {
    let bytes = match sabitori_net::fetch::fetch_bytes(url).await {
        Ok(bytes) => bytes,
        Err(e) => return CacheState::Failed(e),
    };
    // web はブラウザに読ませる (`createImageBitmap` は画面のスレッドの外で
    // 読み込みと縮小をする)。native はこのタスク自体が画面の外にいる。
    #[cfg(target_arch = "wasm32")]
    let decoded = sabitori_net::decode::decode_image_in_browser(&bytes, max_px).await;
    #[cfg(not(target_arch = "wasm32"))]
    let decoded = sabitori_net::decode::decode_image_max(&bytes, max_px);
    match decoded {
        Ok(data) => CacheState::Loaded(data),
        Err(e) => CacheState::Failed(e),
    }
}

/// 鍵にまだ何も無ければ `Loading` を付けて `true`。同じフレーム (や、読み終わる
/// までの後続フレーム) で同じ鍵が何度頼まれても 1 回しか読みに行かない。
fn claim(cache: &Mutex<ImageCache>, key: &str) -> bool {
    let mut c = cache.lock().unwrap();
    if !matches!(c.get(key), CacheState::Missing) {
        return false;
    }
    c.mark_loading(key);
    true
}

/// Build an `ImageCtx` whose `request` closure spawns `fetch_bytes` +
/// decode in the background, writing the result into `pending`.
/// Already-queued keys are skipped via the cache's `Loading` marker.
#[cfg(not(target_arch = "wasm32"))]
pub fn make_image_ctx(
    cache: Arc<Mutex<ImageCache>>,
    pending: PendingQueue,
    rt: tokio::runtime::Handle,
) -> ImageCtx {
    let cache_for_closure = cache.clone();
    let request: Arc<dyn Fn(ImageRequest<'_>) + Send + Sync> = Arc::new(move |req: ImageRequest<'_>| {
        let key = req.key();
        if !claim(&cache_for_closure, &key) {
            return;
        }
        let url = req.url.to_string();
        let max_px = req.max_px;
        let pending = pending.clone();
        rt.spawn(async move {
            let result = load(&url, max_px).await;
            pending.lock().unwrap().push((key, result));
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
    let request: Arc<dyn Fn(ImageRequest<'_>) + Send + Sync> = Arc::new(move |req: ImageRequest<'_>| {
        let key = req.key();
        if !claim(&cache_for_closure, &key) {
            return;
        }
        let url = req.url.to_string();
        let max_px = req.max_px;
        let pending = pending.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = load(&url, max_px).await;
            pending.lock().unwrap().push((key, result));
            // 入力が無いと web のループは描かない。届いたら 1 フレーム起こす
            // (native は毎刻みに `DrawGate::images_arrived` で拾う)。
            crate::web_wake::wake();
        });
    });
    ImageCtx { cache, request }
}
