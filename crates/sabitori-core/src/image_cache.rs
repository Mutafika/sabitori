//! URL-keyed cache of decoded `ImageData`.
//!
//! Lives in sabitori-core (no http/decode deps) so `ViewContext` can hold a
//! reference. Fetch/decode logic is provided by `sabitori-net`.

use std::collections::HashMap;

use crate::element::ImageData;

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

/// URL → decode-state map. Wrap in `Arc<Mutex<_>>` or `Rc<RefCell<_>>`
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
