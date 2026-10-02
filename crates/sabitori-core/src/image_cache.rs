//! URL-keyed cache of decoded `ImageData`.
//!
//! Lives in sabitori-core (no http/decode deps) so `ViewContext` can hold a
//! reference. Fetch/decode logic is provided by `sabitori-net`.

use std::collections::HashMap;

use crate::element::ImageData;

/// 読み込み 1 件の注文 — どの URL を、どの大きさまで縮めて持つか。
///
/// 写真のサムネイルに要るのは数百 px なのに、原寸 (4032×3024 で RGBA 48MB) で
/// 読んで持つと、読み込み・メモリ・GPU のテクスチャが全部原寸ぶんかかる
/// ([#114](https://github.com/Mutafika/sabitori/issues/114))。`max_px` を付けると
/// 読み込んだ直後に縮め、縮めた方だけをキャッシュに置く。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageRequest<'a> {
    pub url: &'a str,
    /// 長い辺の上限 (**物理 px**)。`None` なら原寸。拡大はしない。
    pub max_px: Option<u32>,
}

impl ImageRequest<'_> {
    /// キャッシュとテクスチャの鍵。
    ///
    /// 大きさ違いは別の鍵になるので、一覧のサムネイルと拡大表示で同じ URL を
    /// 別の大きさで並べて使える。原寸は URL そのまま (以前からの鍵と同じ)。
    /// 区切りに空白を使うのは、正しい URL には生の空白が入らず衝突しないから。
    pub fn key(&self) -> String {
        match self.max_px {
            None => self.url.to_string(),
            Some(px) => format!("{} @{px}px", self.url),
        }
    }
}

/// 長い辺が `max_px` に収まる大きさ。縦横比を保ち、拡大はしない。各辺は 1 以上。
///
/// native の縮小と web の `createImageBitmap` の両方がこれで大きさを決める
/// (web だけのコードは CI で走らないので、決める部分はここに置く)。
pub fn fit_within(width: u32, height: u32, max_px: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= max_px || long == 0 {
        return (width, height);
    }
    let scale = max_px as f64 / long as f64;
    let w = ((width as f64 * scale).round() as u32).clamp(1, max_px);
    let h = ((height as f64 * scale).round() as u32).clamp(1, max_px);
    (w, h)
}

/// Lookup state for a URL in the cache.
#[derive(Clone, Debug)]
pub enum CacheState {
    /// URL never requested. The runtime should issue a fetch.
    Missing,
    /// Fetch (or decode) is in progress.
    Loading,
    /// Ready — returned `ImageData` can be cloned into the element tree.
    Loaded(ImageData),
    /// Fetch or decode failed.
    Failed(String),
}

/// Key → decode-state map (鍵は [`ImageRequest::key`])。 Wrap in `Arc<Mutex<_>>` or `Rc<RefCell<_>>`
/// depending on threading needs.
#[derive(Default)]
pub struct ImageCache {
    entries: HashMap<String, CacheState>,
    /// Maximum cache entries. When exceeded, oldest are dropped (LRU-lite).
    /// `0` disables eviction.
    pub max_entries: usize,
    /// Insertion-order tracking for simple eviction.
    order: Vec<String>,
}

impl ImageCache {
    pub fn new() -> Self {
        Self { max_entries: 256, ..Default::default() }
    }

    pub fn get(&self, url: &str) -> CacheState {
        self.entries
            .get(url)
            .cloned()
            .unwrap_or(CacheState::Missing)
    }

    /// Forcefully insert a result (useful after your own fetch pipeline runs).
    pub fn insert(&mut self, url: &str, state: CacheState) {
        if !self.entries.contains_key(url) {
            self.order.push(url.to_string());
        }
        self.entries.insert(url.to_string(), state);
        self.evict();
    }

    /// Mark the URL as loading without starting a fetch (useful when the
    /// caller spawns its own task).
    pub fn mark_loading(&mut self, url: &str) {
        if !self.entries.contains_key(url) {
            self.order.push(url.to_string());
        }
        self.entries
            .insert(url.to_string(), CacheState::Loading);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn evict(&mut self) {
        if self.max_entries == 0 {
            return;
        }
        while self.order.len() > self.max_entries {
            let drop_url = self.order.remove(0);
            self.entries.remove(&drop_url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_size_key_is_the_bare_url() {
        let r = ImageRequest { url: "https://x/a.jpg", max_px: None };
        assert_eq!(r.key(), "https://x/a.jpg");
    }

    #[test]
    fn sizes_get_their_own_keys() {
        let a = ImageRequest { url: "https://x/a.jpg", max_px: Some(400) }.key();
        let b = ImageRequest { url: "https://x/a.jpg", max_px: Some(1600) }.key();
        assert_ne!(a, b);
        assert_ne!(a, "https://x/a.jpg");
    }

    #[test]
    fn fit_within_keeps_aspect_and_never_enlarges() {
        assert_eq!(fit_within(4032, 3024, 400), (400, 300));
        assert_eq!(fit_within(3024, 4032, 400), (300, 400));
        assert_eq!(fit_within(64, 48, 400), (64, 48));
        assert_eq!(fit_within(400, 10, 400), (400, 10));
        // 極端に細長くても 0 px にはしない。
        assert_eq!(fit_within(10000, 1, 100), (100, 1));
    }
}
