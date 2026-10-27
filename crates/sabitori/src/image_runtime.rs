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

/// feature `net` が無いときに、読めなかった画像として持たせる理由。
#[cfg(not(feature = "net"))]
const NET_DISABLED: &str = "sabitori: URL の画像を取りに行くには feature \"net\" が要る \
     (Cargo.toml で `sabitori = { ..., features = [\"net\"] }`)";

/// feature `net` が無いので取りに行かない。黙って空にすると「画像が出ない」
/// 理由が分からないので、最初の 1 回だけ書き方をログに出す。
#[cfg(not(feature = "net"))]
async fn load(_url: &str, _max_px: Option<u32>) -> CacheState {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| log::warn!("{NET_DISABLED}"));
    CacheState::Failed(NET_DISABLED.to_string())
}

/// 取って、読んで (縮める指定があれば縮めて)、キャッシュに置く形にする。
#[cfg(feature = "net")]
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// ランタイムと同じ組み立てで 1 枚頼み、結果がキャッシュに入るまで待つ。
    fn request_and_wait(url: &str) -> CacheState {
        let cache = Arc::new(Mutex::new(ImageCache::new()));
        let pending: PendingQueue = Arc::new(Mutex::new(Vec::new()));
        let rt = crate::tasks::runtime_handle().expect("tokio runtime");
        let ctx = make_image_ctx(cache.clone(), pending.clone(), rt);
        (ctx.request)(ImageRequest { url, max_px: None });
        let deadline = Instant::now() + Duration::from_secs(10);
        while pending.lock().unwrap().is_empty() {
            assert!(Instant::now() < deadline, "10 秒待っても結果が来ない");
            std::thread::sleep(Duration::from_millis(10));
        }
        drain_pending(&cache, &pending);
        cache.lock().unwrap().get(url)
    }

    /// `net` が無いと取りに行かず、読めなかった画像として**書き方を**持たせる。
    /// `Loading` のまま残ったり理由が空だったりすると、画像が出ない理由が分からない。
    #[cfg(not(feature = "net"))]
    #[test]
    fn without_net_a_url_image_fails_and_says_how_to_enable_it() {
        match request_and_wait("https://example.test/a.png") {
            CacheState::Failed(why) => assert!(why.contains("features = [\"net\"]"), "{why}"),
            _ => panic!("読めなかった画像になっていない"),
        }
    }

    /// `net` があれば取りに行って読む。外へは出ず、手元に立てた HTTP で確かめる。
    #[cfg(feature = "net")]
    #[test]
    fn with_net_a_url_image_is_fetched_and_decoded() {
        use image::ImageEncoder;
        use std::io::{Read, Write};

        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[255u8; 2 * 3 * 4], 2, 3, image::ExtendedColorType::Rgba8)
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                png.len()
            );
            s.write_all(head.as_bytes()).unwrap();
            s.write_all(&png).unwrap();
        });

        match request_and_wait(&format!("http://127.0.0.1:{port}/a.png")) {
            CacheState::Loaded(data) => assert_eq!((data.width, data.height), (2, 3)),
            CacheState::Failed(why) => panic!("読めなかった: {why}"),
            _ => panic!("読み終わっていない"),
        }
    }
}
