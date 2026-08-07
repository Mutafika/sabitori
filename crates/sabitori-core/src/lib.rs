mod color;
mod geometry;
mod theme;
pub mod element;
pub mod render_list;
pub mod build;
pub mod tui;
pub mod forms;
pub mod image_cache;

pub use color::Color;
pub use geometry::{Corners, Edges, Point, Rect, Size, TextMetrics};
pub use theme::AppTheme;

// Re-export key element API items at crate root for convenience.
pub use element::{
    arc, div, text, button, image, ArcKind, Cursor, Element, ElementKind, HighlightSpec, ImageData,
    LinkRange, ObjectFit, Typography,
};
pub use element::{Dimension, Px, Percent, Auto, DimensionExt};
pub use element::{
    EasingFn, StateStyle, Transition, TransitionKind, TransitionProperty,
};
pub use render_list::{RenderCommand, RenderList, RectDraw, RingDraw, TextDraw, ImageDraw};
pub use build::{build_tree, build_tree_measured, BuildResult, HitRegion, ScrollMeasure, TextMeasure};
pub use tui::{
    block, hsep, vsep, status_bar, status_segment, key_hint, BlockBuilder,
    typewriter, spinner, progress_bar, gradient_text, wave_text, easing_bar,
    scroll_container,
    context_menu, context_menu_item, menu_separator, MenuItem,
    tooltip_popup,
};
pub use forms::{
    text_input as form_text_input, checkbox, radio, slider, labeled_slider, dropdown_trigger,
    segment_control, numeric_input, collapsing_header, collapsing_section,
    progress_bar as form_progress_bar, labeled_progress_bar,
};

// ---------------------------------------------------------------------------
// TooltipInfo
// ---------------------------------------------------------------------------

/// Information about an active tooltip to display.
#[derive(Clone, Debug)]
pub struct TooltipInfo {
    pub text: String,
    pub x: f32,
    pub y: f32,
}

// ---------------------------------------------------------------------------
// DragInfo — active drag state
// ---------------------------------------------------------------------------

/// Information about an active drag operation.
#[derive(Clone, Debug)]
pub struct DragInfo {
    /// The drag payload ID (from `.draggable("id")`).
    pub data: String,
    /// ID of the source element being dragged.
    pub source_id: Option<String>,
    /// ID of the drop zone currently under cursor (if any).
    pub over_drop_zone: Option<String>,
}

// ---------------------------------------------------------------------------
// ViewContext — passed to DeclarativeApp::view()
// ---------------------------------------------------------------------------

/// Handle the runtime hands to `ViewContext` so `image_url()` can consult the
/// shared cache and kick off fetches without the app wiring it up.
#[derive(Clone)]
pub struct ImageCtx {
    pub cache: std::sync::Arc<std::sync::Mutex<image_cache::ImageCache>>,
    /// Called synchronously by `image_url()` for URLs not yet in the cache.
    /// The closure is responsible for spawning an async fetch + decode and
    /// writing the result back into the cache when it completes.
    pub request: std::sync::Arc<dyn Fn(&str) + Send + Sync>,
}

/// Coarse responsive breakpoint bucket derived from the viewport width.
///
/// Apps `match ctx.size_class()` to reflow their layout (e.g. a 3-pane reader
/// collapsing to a single pane) instead of scattering pixel thresholds across
/// call sites. The buckets follow the common phone / tablet / desktop split;
/// the exact cut points live in [`SizeClass::COMPACT_MAX`] and
/// [`SizeClass::MEDIUM_MAX`] so an app can reason about them explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeClass {
    /// Phone portrait / narrow window — show one pane at a time. `width < 640`.
    Compact,
    /// Tablet portrait / split view — two panes fit side by side. `640 ≤ width < 1040`.
    Medium,
    /// Tablet landscape / desktop — full multi-pane layout. `width ≥ 1040`.
    Expanded,
}

impl SizeClass {
    /// Upper bound (exclusive) of [`SizeClass::Compact`], in logical points.
    pub const COMPACT_MAX: f32 = 640.0;
    /// Upper bound (exclusive) of [`SizeClass::Medium`], in logical points.
    pub const MEDIUM_MAX: f32 = 1040.0;

    /// Bucket a viewport width into a size class.
    pub fn from_width(width: f32) -> SizeClass {
        if width < Self::COMPACT_MAX {
            SizeClass::Compact
        } else if width < Self::MEDIUM_MAX {
            SizeClass::Medium
        } else {
            SizeClass::Expanded
        }
    }

    /// How many side-by-side panes this class can comfortably host (1 / 2 / 3).
    pub fn panes(self) -> u8 {
        match self {
            SizeClass::Compact => 1,
            SizeClass::Medium => 2,
            SizeClass::Expanded => 3,
        }
    }
}

/// Context passed to `DeclarativeApp::view()` each frame.
/// Contains viewport info, mouse state, and hovered element ID.
pub struct ViewContext {
    pub width: f32,
    pub height: f32,
    /// ID of the currently hovered element (if any).
    pub hovered: Option<String>,
    /// ID of the currently focused element (if any).
    pub focused: Option<String>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// Whether the Shift key is currently held.
    pub shift_held: bool,
    /// Whether the Command (Meta/Super) key is currently held.
    pub cmd_held: bool,
    /// Scroll states: id → current offsets + viewport + content extents on both axes.
    /// Use `scroll_info("id")` for convenient access.
    pub scroll_states: std::collections::HashMap<String, ScrollInfo>,
    /// Active tooltip info (if any element's tooltip is showing).
    pub tooltip: Option<TooltipInfo>,
    /// Active drag operation info (if any).
    pub drag: Option<DragInfo>,
    /// Framework-level theme with semantic color names.
    pub theme: AppTheme,
    /// Presence animation progress values: id -> progress (0.0-1.0).
    /// Use `presence()` to query exit animation progress for keeping elements rendered.
    pub presence: std::collections::HashMap<String, f32>,
    /// Shared image cache + spawner. `None` until the runtime wires it up.
    pub images: Option<ImageCtx>,
    /// Advance width of one monospace cell at font-size 1.0 — i.e. the active
    /// `.mono()` face's `measure("0…", s) / n / s`. Multiply by your font px for
    /// an exact cell width (terminals, code grids tile perfectly for any face).
    /// `0.6` is a generic fallback before the runtime measures.
    pub mono_advance: f32,
}

/// Scroll state snapshot for a scroll container, exposed via [`ViewContext`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollInfo {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub content_width: f32,
    pub content_height: f32,
}

impl ViewContext {
    /// Get scroll info for a scroll container by ID.
    pub fn scroll_info(&self, id: &str) -> Option<ScrollInfo> {
        self.scroll_states.get(id).copied()
    }

    /// Whether the scroll container's viewport is at (or within `threshold` px of) the bottom.
    pub fn scroll_at_bottom(&self, id: &str, threshold: f32) -> bool {
        match self.scroll_info(id) {
            Some(info) => {
                let max_scroll = (info.content_height - info.viewport_height).max(0.0);
                info.scroll_y + threshold >= max_scroll
            }
            None => true, // not yet laid out — treat as bottom so first frame snaps
        }
    }

    /// Compute the visible item range for virtual scrolling.
    /// Returns (first_visible, count) given an item height.
    pub fn visible_range(&self, id: &str, item_height: f32) -> (usize, usize) {
        if let Some(info) = self.scroll_info(id) {
            let first = (info.scroll_y / item_height).floor().max(0.0) as usize;
            let count = (info.viewport_height / item_height).ceil() as usize + 2;
            (first, count)
        } else {
            (0, 100) // fallback
        }
    }

    // -- Responsive layout --

    /// The current [`SizeClass`], bucketed from [`ViewContext::width`].
    /// `match` on this to reflow the layout to the window size.
    pub fn size_class(&self) -> SizeClass {
        SizeClass::from_width(self.width)
    }

    /// Convenience: is the viewport in the single-pane [`SizeClass::Compact`] range?
    pub fn is_compact(&self) -> bool {
        self.size_class() == SizeClass::Compact
    }

    /// Convenience: is the viewport in the full multi-pane [`SizeClass::Expanded`] range?
    pub fn is_expanded(&self) -> bool {
        self.size_class() == SizeClass::Expanded
    }

    // -- Presence animation helpers --

    /// Get presence animation progress for an element (0.0 = gone, 1.0 = fully present).
    /// Useful for exit animations: keep rendering while progress > 0.
    pub fn presence(&self, id: &str) -> f32 {
        self.presence.get(id).copied().unwrap_or(0.0)
    }

    // -- Drag & Drop helpers --

    /// Whether a drag operation is currently active.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Get the drag payload data (if dragging).
    pub fn drag_data(&self) -> Option<&str> {
        self.drag.as_ref().map(|d| d.data.as_str())
    }

    /// Get the ID of the drop zone currently under the cursor (if any).
    pub fn drag_over(&self) -> Option<&str> {
        self.drag.as_ref().and_then(|d| d.over_drop_zone.as_deref())
    }

    // -- Async image loading --

    /// Returns an [`element::Element`] displaying the image at `url`. First
    /// call for a URL kicks off a background fetch + decode; subsequent
    /// calls return the decoded image once ready, and a transparent
    /// placeholder in the meantime.
    ///
    /// Chain `.w()` / `.h()` / `.rounded_px()` / `.object_fit()` etc. on
    /// the result — the returned element honours the same builder API
    /// whether it's loaded or not, so layout stays stable across loads.
    ///
    /// Panics if the runtime hasn't wired up `ImageCtx`. Won't happen for
    /// apps driven by `sabitori::run_declarative`; if you build your own
    /// driver, fall back to `element::image(key, data)` with your own
    /// cache, or populate `ViewContext::images`.
    pub fn image_url(&self, url: &str) -> element::Element {
        let images = self
            .images
            .as_ref()
            .expect("ViewContext::image_url requires a runtime that wires up ImageCtx");
        let state = images.cache.lock().unwrap().get(url);
        match state {
            image_cache::CacheState::Loaded(data) => element::image(url, data),
            image_cache::CacheState::Missing => {
                (images.request)(url);
                element::div()
            }
            image_cache::CacheState::Loading | image_cache::CacheState::Failed(_) => {
                element::div()
            }
        }
    }

    /// Raw-data variant of [`Self::image_url`]. Same fetch-on-miss behaviour,
    /// but returns `Option<ImageData>` instead of a ready-to-use `Element`.
    /// Useful for adapters (e.g., the markdown renderer's resolver callback)
    /// that need the pixel data directly.
    pub fn image_data(&self, url: &str) -> Option<element::ImageData> {
        let images = self.images.as_ref()?;
        let state = images.cache.lock().unwrap().get(url);
        match state {
            image_cache::CacheState::Loaded(data) => Some(data),
            image_cache::CacheState::Missing => {
                (images.request)(url);
                None
            }
            image_cache::CacheState::Loading | image_cache::CacheState::Failed(_) => None,
        }
    }
}
