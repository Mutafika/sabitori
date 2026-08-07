//! Declarative UI element builders.
//!
//! Provides a builder-pattern API for constructing UI trees:
//!
//! ```ignore
//! div()
//!     .w(Px(200.0)).h_full()
//!     .bg(theme.surface)
//!     .rounded(Px(8.0))
//!     .p(Px(16.0)).gap(8.0)
//!     .flex_col()
//!     .children([
//!         text("Hello").font_size(24.0).color(theme.text_primary),
//!         button("Click").accent(theme.primary).on_click(|| log("clicked")),
//!     ])
//! ```

use std::sync::Arc;
use crate::{Color, Corners, Point};

// ---------------------------------------------------------------------------
// Dimension (local to avoid circular dep with sabitori-style)
// ---------------------------------------------------------------------------

/// CSS-like dimension value for the element builder API.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
}

impl Default for Dimension {
    fn default() -> Self {
        Self::Auto
    }
}

/// Shorthand constructors.
impl Dimension {
    pub fn px(v: f32) -> Self {
        Self::Px(v)
    }

    pub fn pct(v: f32) -> Self {
        Self::Percent(v)
    }
}

/// Convenience: `200.0.px()` and `50.0.pct()`.
pub trait DimensionExt {
    fn px(self) -> Dimension;
    fn pct(self) -> Dimension;
}

impl DimensionExt for f32 {
    fn px(self) -> Dimension {
        Dimension::Px(self)
    }
    fn pct(self) -> Dimension {
        Dimension::Percent(self)
    }
}

impl DimensionExt for i32 {
    fn px(self) -> Dimension {
        Dimension::Px(self as f32)
    }
    fn pct(self) -> Dimension {
        Dimension::Percent(self as f32)
    }
}

// Allow `Px(200.0)` shorthand at call site.
pub use Dimension::Auto;
pub use Dimension::Percent;
pub use Dimension::Px;

// ---------------------------------------------------------------------------
// Enums for layout properties
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignItems {
    #[default]
    Stretch,
    Start,
    End,
    Center,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

// ---------------------------------------------------------------------------
// Edge dimensions (padding / margin)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeDimensions {
    pub top: Dimension,
    pub right: Dimension,
    pub bottom: Dimension,
    pub left: Dimension,
}

impl EdgeDimensions {
    pub fn all(v: Dimension) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }

    pub fn axes(vertical: Dimension, horizontal: Dimension) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
}

// ---------------------------------------------------------------------------
// Box shadow
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Point,
    pub blur: f32,
    pub spread: f32,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            offset: Point::ZERO,
            blur: 0.0,
            spread: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Image types
// ---------------------------------------------------------------------------

/// Raw decoded image data. Uses Arc for cheap cloning in view() calls.
#[derive(Clone, Debug)]
pub struct ImageData {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

impl ImageData {
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self { rgba: Arc::new(rgba), width, height }
    }
}

/// How an image fills its container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    /// Scale to fill, cropping if necessary. Preserves aspect ratio.
    #[default]
    Cover,
    /// Scale to fit entirely within the box. Preserves aspect ratio.
    Contain,
    /// Stretch to fill (ignores aspect ratio).
    Fill,
}

// ---------------------------------------------------------------------------
// ElementStyle — resolved style for a single element
// ---------------------------------------------------------------------------

/// Per-range background highlight for a text element — Word-style "find in
/// page" highlight. `ranges` are `(byte_start, byte_end)` offsets into the
/// text's content; the runtime paints a background rect behind the glyphs of
/// each range, drawn below the text and above the element background. It reuses
/// the same glyph-hitbox layout the text selection system uses, so highlights
/// track wrapping and CJK shaping exactly. `current` (an index into `ranges`)
/// is painted in `current_color` instead of `color` — used to accent the
/// active match in a find bar. Set via `Element::highlight`.
#[derive(Clone, Debug)]
pub struct HighlightSpec {
    /// Byte ranges `(start, end)` into the text content to highlight.
    pub ranges: Vec<(usize, usize)>,
    /// Background color for every range except `current`.
    pub color: Color,
    /// Index into `ranges` of the active match, or `None`.
    pub current: Option<usize>,
    /// Background color for the `current` match.
    pub current_color: Color,
}

impl Default for HighlightSpec {
    fn default() -> Self {
        Self {
            ranges: Vec::new(),
            color: Color::TRANSPARENT,
            current: None,
            current_color: Color::TRANSPARENT,
        }
    }
}

/// One clickable + hoverable byte range inside a text element's content.
/// Resolved to on-screen rects through the same glyph-hitbox layout the text
/// selection / [`HighlightSpec`] systems use, so it tracks wrapping and CJK
/// shaping exactly. A pointer press inside the range dispatches
/// `DeclarativeApp::on_click(id)`; `tooltip` (if set) shows on hover; the range
/// is underlined in `color`. Set via `Element::link_ranges` — used for in-body
/// 条文リンク (citation links) in flowing text.
#[derive(Clone, Debug)]
pub struct LinkRange {
    /// Byte offset range `[start, end)` into the (untruncated) text content.
    pub start: usize,
    pub end: usize,
    /// Dispatched via `DeclarativeApp::on_click` when the range is clicked.
    pub id: String,
    /// Hover-preview text (shown through the tooltip system). `None` = no tooltip.
    pub tooltip: Option<String>,
    /// Underline color for the range.
    pub color: Color,
}

/// Complete resolved style for one element.
#[derive(Clone, Debug)]
pub struct ElementStyle {
    // Layout
    pub position: Position,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    /// Flex base size along the parent's main axis. `Auto` (default) sizes
    /// from the width/height property, falling back to content size — the
    /// CSS `flex-basis: auto` behavior. `flex_1()` sets this to `Px(0.0)`
    /// (Tailwind `flex-1` = `flex: 1 1 0%`) so growing items split the
    /// *free* space instead of starting from their content size.
    pub flex_basis: Dimension,
    pub gap: f32,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub min_height: Dimension,
    pub max_width: Dimension,
    pub max_height: Dimension,
    pub padding: EdgeDimensions,
    pub margin: EdgeDimensions,
    pub overflow: Overflow,
    /// Inset for absolute positioning (top, right, bottom, left).
    pub inset_top: Dimension,
    pub inset_right: Dimension,
    pub inset_bottom: Dimension,
    pub inset_left: Dimension,
    /// Scroll offset in logical pixels (x, y). Applied when overflow is Scroll.
    pub scroll_x: f32,
    pub scroll_y: f32,

    // Visual
    pub background: Color,
    /// Gradient end color. When set with gradient_angle != 0, creates a linear gradient.
    pub gradient_end: Color,
    /// Gradient angle in radians (0 = left-to-right, PI/2 = top-to-bottom).
    pub gradient_angle: f32,
    pub corner_radius: Corners<f32>,
    pub border_color: Color,
    pub border_width: f32,
    pub shadow: Option<BoxShadow>,
    pub opacity: f32,
    pub object_fit: ObjectFit,
    /// 回転角 (ラジアン)。正 = 画面上時計回り (Y 下向き座標系)。
    ///
    /// 矩形は**中心**まわり (線描画 = 回転した細 rect 用)、テキストは
    /// **原点** (レイアウト後の左上) まわりに回る。CAD の注記が挿入点まわりに
    /// 回る仕様に合わせたため。背景付きの要素を回すと箱とラベルはずれる。
    pub rotation: f32,
    /// Visual-only X offset, applied after layout. Does not push siblings —
    /// taffy still measures the element at its laid-out slot, but the rect
    /// (and its hit region) is rendered shifted by this many px. Drives
    /// hover-spring "lift / pull" effects without disturbing flex flow.
    pub translate_x: f32,
    /// Visual-only Y offset. See `translate_x` for semantics.
    pub translate_y: f32,

    // Text (inherited)
    pub color: Color,
    pub font_size: f32,
    pub bold: bool,
    pub monospace: bool,
    /// Specific font family for this element's text, overriding the generic
    /// (and any app-level preferred) family at shaping time. `None` (default)
    /// keeps the normal resolution: monospace/sans-serif generics, optionally
    /// redirected by the renderer's preferred families. Set it to render text
    /// in a *named* face regardless of app defaults — e.g. a font picker
    /// previewing each row in its own font (Word-style).
    pub font_family: Option<String>,
    /// Explicit font weight (100–900) overriding `bold`. `None` (default)
    /// derives from `bold` (700 when bold, else 400). Set via `.font_weight(n)`.
    pub font_weight: Option<u16>,
    /// Extra inter-glyph tracking in logical px (`.letter_spacing(px)`).
    /// `0.0` (default) = the font's natural advances. Negative tightens.
    pub letter_spacing: f32,
    /// Line height as a multiple of `font_size` (`.line_height(mult)`).
    /// `None` (default) = 1.4, the framework-wide default.
    pub line_height: Option<f32>,
    /// Maximum visible line count for wrapped text. `None` (default)
    /// means unlimited — wrap to as many lines as the content needs
    /// (Excel-cell style). `Some(n)` clips to `n` lines and appends
    /// `…` at the end of line `n` if more content follows. Combined
    /// with the container's `max_width` (or its computed width when
    /// inside a sized parent), this gives CSS `line-clamp` semantics
    /// for declarative apps.
    pub max_lines: Option<u32>,
    /// Framework-drawn scrollbar thumb color for `Overflow::Scroll`
    /// containers (`.scrollbar(color)`). `None` (default) draws nothing.
    /// When set, a thin rounded thumb is drawn at the container's right
    /// edge whenever the content overflows vertically — position/size from
    /// the same animated scroll offset the content renders with. Indicator
    /// only: it adds no hit region, so click/wheel routing is untouched.
    pub scrollbar_thumb: Option<Color>,
    /// Per-range background highlights for this element's text. Empty
    /// (default) draws nothing. Only text/button draws read it. Set via
    /// `.highlight(HighlightSpec { .. })`, which appends — see there for why
    /// this is a list rather than a single spec.
    pub highlight: Vec<HighlightSpec>,
    /// Clickable/hoverable byte ranges in a text element (in-body links).
    /// `.link_ranges(..)`.
    pub link_ranges: Option<Vec<LinkRange>>,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            position: Position::Relative,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Stretch,
            justify_content: JustifyContent::Start,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            gap: 0.0,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: EdgeDimensions::default(),
            margin: EdgeDimensions::all(Dimension::Px(0.0)),
            overflow: Overflow::Visible,
            inset_top: Dimension::Auto,
            inset_right: Dimension::Auto,
            inset_bottom: Dimension::Auto,
            inset_left: Dimension::Auto,
            scroll_x: 0.0,
            scroll_y: 0.0,
            background: Color::TRANSPARENT,
            gradient_end: Color::TRANSPARENT,
            gradient_angle: 0.0,
            corner_radius: Corners::all(0.0),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            shadow: None,
            opacity: 1.0,
            object_fit: ObjectFit::default(),
            rotation: 0.0,
            translate_x: 0.0,
            translate_y: 0.0,
            color: Color::WHITE,
            font_size: 14.0,
            bold: false,
            monospace: false,
            font_family: None,
            font_weight: None,
            letter_spacing: 0.0,
            line_height: None,
            max_lines: None,
            scrollbar_thumb: None,
            highlight: Vec::new(),
            link_ranges: None,
        }
    }
}

/// Extended typographic controls layered on top of `font_size` / `bold` /
/// `monospace`. Bundled into one value so the shaping path (measure + render)
/// threads a single param instead of three loose ones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    /// Explicit weight (100–900). `None` derives from `bold`
    /// (700 when bold, else 400).
    pub weight: Option<u16>,
    /// Extra tracking between glyphs, in logical px. `0.0` = natural advances.
    pub letter_spacing: f32,
    /// Line height as a multiple of the font size. `None` = 1.4.
    pub line_height: Option<f32>,
}

impl Default for Typography {
    fn default() -> Self {
        Self { weight: None, letter_spacing: 0.0, line_height: None }
    }
}

impl Typography {
    /// Numeric weight to shape with, honoring `bold` when `weight` is unset.
    pub fn resolved_weight(&self, bold: bool) -> u16 {
        self.weight.unwrap_or(if bold { 700 } else { 400 })
    }

    /// Absolute line height in logical px for a given font size.
    pub fn line_height_px(&self, font_size: f32) -> f32 {
        font_size * self.line_height.unwrap_or(1.4)
    }
}

impl ElementStyle {
    /// Bundle this style's extended typographic controls for the shaper.
    pub fn typography(&self) -> Typography {
        Typography {
            weight: self.font_weight,
            letter_spacing: self.letter_spacing,
            line_height: self.line_height,
        }
    }
}

// ---------------------------------------------------------------------------
// Event callbacks
// ---------------------------------------------------------------------------

/// Boxed event callback. Use `Box<dyn FnMut()>` for click handlers etc.
pub type EventHandler = Box<dyn FnMut() + 'static>;

// ---------------------------------------------------------------------------
// Transition & interactive state styles
// ---------------------------------------------------------------------------

/// Which property to animate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransitionProperty {
    Background,
    BorderColor,
    Shadow,
    Opacity,
    Transform,
    All,
}

/// Animation type for transitions.
#[derive(Clone, Copy, Debug)]
pub enum TransitionKind {
    /// Duration-based easing.
    Easing { duration: f32, function: EasingFn },
    /// Physics-based spring.
    Spring { stiffness: f32, damping: f32 },
}

impl Default for TransitionKind {
    fn default() -> Self {
        Self::Spring {
            stiffness: 200.0,
            damping: 20.0,
        }
    }
}

/// CSS-style easing (subset, inlined to avoid cross-crate dep).
#[derive(Clone, Copy, Debug)]
pub enum EasingFn {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

/// A single transition declaration.
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    pub property: TransitionProperty,
    pub kind: TransitionKind,
}

/// Style overrides for hover / active / focus states.
/// Only `Some` fields override the base style.
#[derive(Clone, Debug, Default)]
pub struct StateStyle {
    pub background: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub shadow: Option<Option<BoxShadow>>,
    pub opacity: Option<f32>,
    pub corner_radius: Option<Corners<f32>>,
    /// Scale transform (1.0 = normal).
    pub scale: Option<f32>,
    /// X offset (for slide animations).
    pub translate_x: Option<f32>,
    /// Y offset (for slide animations).
    pub translate_y: Option<f32>,
    /// Gap override.
    pub gap: Option<f32>,
    /// Width override.
    pub width: Option<Dimension>,
    /// Height override.
    pub height: Option<Dimension>,
    /// Padding override (all sides).
    pub padding: Option<EdgeDimensions>,
    /// Text color override.
    pub color: Option<Color>,
    /// Font size override.
    pub font_size: Option<f32>,
}

impl StateStyle {
    /// Merge this state style onto a base ElementStyle, returning a new resolved style.
    pub fn apply_to(&self, base: &ElementStyle) -> ElementStyle {
        let mut s = base.clone();
        if let Some(bg) = self.background {
            s.background = bg;
        }
        if let Some(bc) = self.border_color {
            s.border_color = bc;
        }
        if let Some(bw) = self.border_width {
            s.border_width = bw;
        }
        if let Some(ref shadow) = self.shadow {
            s.shadow = shadow.clone();
        }
        if let Some(op) = self.opacity {
            s.opacity = op;
        }
        if let Some(cr) = self.corner_radius {
            s.corner_radius = cr;
        }
        if let Some(g) = self.gap {
            s.gap = g;
        }
        if let Some(w) = self.width {
            s.width = w;
        }
        if let Some(h) = self.height {
            s.height = h;
        }
        if let Some(p) = self.padding {
            s.padding = p;
        }
        if let Some(c) = self.color {
            s.color = c;
        }
        if let Some(fs) = self.font_size {
            s.font_size = fs;
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Element — the core tree node
// ---------------------------------------------------------------------------

/// A declarative UI element. Constructed via `div()`, `text()`, or `button()`.
pub struct Element {
    pub kind: ElementKind,
    pub style: ElementStyle,
    pub children: Vec<Element>,
    /// Unique identifier for event dispatch (click/hover).
    pub id: Option<String>,
    pub on_click: Option<EventHandler>,
    pub on_hover: Option<EventHandler>,
    /// Whether this element can receive focus.
    pub focusable: bool,
    /// Style overrides when hovered.
    pub hover_style: Option<StateStyle>,
    /// Style overrides when pressed/active.
    pub active_style: Option<StateStyle>,
    /// Transition declarations.
    pub transitions: Vec<Transition>,
    /// When true, this element (and all its children) renders on the overlay
    /// layer — drawn after all base-layer content so it appears on top.
    pub overlay: bool,
    /// Tooltip text shown on hover after a short delay.
    pub tooltip: Option<String>,
    /// Drag payload data. When set, this element can be dragged.
    pub drag_data: Option<String>,
    /// Whether this element is a drop zone (accepts dragged items).
    pub drop_zone: bool,
    /// Whether this element should animate in/out when added/removed from the tree.
    /// Requires `.id()` to be set for tracking.
    pub animate_presence: bool,
    /// Mouse cursor to display while hovering this element. `None` means
    /// "no opinion" — the runtime falls back to whatever the parent
    /// requests, or to the platform default if no ancestor opted in.
    /// Use `.cursor(Cursor::Pointer)` for clickable widgets that should
    /// show the hand cursor (buttons, links, tiles); leave it `None`
    /// for menu rows / panel bodies that should keep the default arrow
    /// per macOS HIG.
    pub cursor: Option<Cursor>,
    /// Opt this element (and its whole subtree) out of text selection —
    /// the CSS `user-select: none` equivalent. Text under a `no_select`
    /// ancestor never becomes a selection anchor/head, is never painted
    /// with the selection background, and is skipped by clipboard extract.
    ///
    /// Use it on chrome — toolbars, sidebars, table headers, status bars —
    /// so that dragging inside a panel doesn't smear a blue highlight over
    /// every label. Prose stays selectable because you simply don't set it
    /// there. Inherits down, so one call on the panel root covers it.
    ///
    /// Button labels are non-selectable regardless of this flag: a control
    /// label is not content.
    pub no_select: bool,
}

/// Mouse-cursor preference for an [`Element`]. Mirrors the platform
/// values most apps actually need; runtime backends translate to their
/// native cursor type (winit `CursorIcon`, NSCursor, etc.). Add new
/// variants here as concrete needs arise rather than mirroring the full
/// matrix of every platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// Default arrow cursor.
    Default,
    /// Hand cursor for clickable elements (buttons, links).
    Pointer,
    /// I-beam cursor for text input regions.
    Text,
    /// Crosshair cursor for selection / picking.
    Crosshair,
    /// "Not allowed" cursor for disabled / forbidden actions.
    NotAllowed,
    /// Horizontal resize (`ew-resize`) cursor — drag-to-adjust affordance
    /// for numeric inputs and split panes.
    ResizeEw,
}

#[derive(Clone, Debug)]
pub enum ElementKind {
    Div,
    Text { content: String },
    Button { label: String, accent: Option<Color> },
    Image { key: String, data: ImageData },
    /// Arc / ring segment — SDF-rasterized donut sector with separate
    /// active "fill" and inactive "track" colors. The element's layout
    /// rect defines the arc's bounding box; the arc fits inside the
    /// largest centered square. Use for tachometer gauges, activity
    /// rings, progress wheels.
    Arc(ArcKind),
    /// Polyline — an open sequence of connected segments, SDF-rasterized
    /// as capsules (see `line.wgsl`). Points are logical px relative to
    /// the element's layout box origin. Use for charts, sparklines,
    /// connectors.
    Polyline(PolylineKind),
}

/// Layout-relative polyline parameters. Wrapped inside
/// [`ElementKind::Polyline`]. Points are in logical px relative to the
/// element's layout-box origin, so a chart widget can size a box and
/// plot into its local `0..w × 0..h` space.
#[derive(Clone, Debug)]
pub struct PolylineKind {
    /// Vertices in logical px, relative to the element's layout box origin.
    pub points: Vec<(f32, f32)>,
    /// Stroke width (logical px).
    pub width: f32,
    /// Stroke color.
    pub color: Color,
}

/// Layout-independent arc parameters. Wrapped inside [`ElementKind::Arc`].
#[derive(Clone, Copy, Debug)]
pub struct ArcKind {
    /// Active fill fraction in `[0, 1]`. Pixels in the angular range
    /// `[start_angle, start_angle + value * sweep_angle]` use
    /// `fill_color`; the remainder uses `track_color`.
    pub value: f32,
    /// First angle of the full arc (radians, screen-space convention:
    /// y grows down, so `0.75π` is bottom-left, `1.5π` is straight up).
    pub start_angle: f32,
    /// Total angular sweep of the arc (radians).
    pub sweep_angle: f32,
    /// Donut band width (logical px). Inner radius is computed as
    /// `outer_radius - thickness` where `outer_radius` is half the
    /// element's smaller layout dimension.
    pub thickness: f32,
    /// Color of the active fill arc.
    pub fill_color: Color,
    /// Color of the inactive track arc.
    pub track_color: Color,
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Element")
            .field("kind", &self.kind)
            .field("style", &self.style)
            .field("children", &self.children)
            .field("id", &self.id)
            .field("on_click", &self.on_click.as_ref().map(|_| ".."))
            .field("on_hover", &self.on_hover.as_ref().map(|_| ".."))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

/// Create a div element (generic container).
pub fn div() -> Element {
    Element {
        kind: ElementKind::Div,
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create a text element.
pub fn text(content: impl Into<String>) -> Element {
    Element {
        kind: ElementKind::Text { content: content.into() },
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create a polyline element — an open sequence of connected line
/// segments, SDF-rasterized with round joints. Points are logical px
/// relative to the element's layout box (set width/height so it has a
/// box, or place points in absolute-ish coords within a full-size box).
/// Use for charts, sparklines, connectors. Set vertices with `.points`
/// and stroke with `.stroke_width` / `.stroke_color`.
pub fn polyline() -> Element {
    Element {
        kind: ElementKind::Polyline(PolylineKind {
            points: Vec::new(),
            width: 1.5,
            color: Color::TRANSPARENT,
        }),
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create an arc / ring element.
///
/// Defaults: 270° sweep starting at the bottom-left (tachometer
/// orientation), `value = 0.0`, 8-px thickness, transparent fill +
/// track. Tweak via the builder methods (`.arc_value`,
/// `.arc_thickness`, `.arc_colors`, `.arc_sweep`).
///
/// The element has a default size of 0×0; set width/height (or use a
/// flex parent) so it gets a real bounding box. The renderer fits
/// the arc inside the largest centered square of that bounding box.
pub fn arc() -> Element {
    Element {
        kind: ElementKind::Arc(ArcKind {
            value: 0.0,
            start_angle: std::f32::consts::PI * 0.75,
            sweep_angle: std::f32::consts::PI * 1.5,
            thickness: 8.0,
            fill_color: Color::TRANSPARENT,
            track_color: Color::TRANSPARENT,
        }),
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create an image element.
pub fn image(key: impl Into<String>, data: ImageData) -> Element {
    Element {
        kind: ElementKind::Image { key: key.into(), data },
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create a button element with default interactive styles.
pub fn button(label: impl Into<String>) -> Element {
    let mut style = ElementStyle::default();
    style.flex_direction = FlexDirection::Row;
    style.align_items = AlignItems::Center;
    style.justify_content = JustifyContent::Center;
    style.padding = EdgeDimensions::axes(Dimension::Px(8.0), Dimension::Px(16.0));
    style.corner_radius = Corners::all(6.0);

    Element {
        kind: ElementKind::Button { label: label.into(), accent: None },
        style,
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        hover_style: None,
        active_style: None,
        transitions: vec![Transition {
            property: TransitionProperty::All,
            kind: TransitionKind::default(),
        }],
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

// ---------------------------------------------------------------------------
// Builder methods (chainable)
// ---------------------------------------------------------------------------

impl Element {
    // -- Sizing --

    /// Set width.
    pub fn w(mut self, d: Dimension) -> Self {
        self.style.width = d;
        self
    }

    /// Set height.
    pub fn h(mut self, d: Dimension) -> Self {
        self.style.height = d;
        self
    }

    /// `width: 100%`
    pub fn w_full(mut self) -> Self {
        self.style.width = Dimension::Percent(100.0);
        self
    }

    /// `height: 100%`
    pub fn h_full(mut self) -> Self {
        self.style.height = Dimension::Percent(100.0);
        self
    }

    /// Set min-width.
    pub fn min_w(mut self, d: Dimension) -> Self {
        self.style.min_width = d;
        self
    }

    /// Set min-height.
    pub fn min_h(mut self, d: Dimension) -> Self {
        self.style.min_height = d;
        self
    }

    /// Set max-width.
    pub fn max_w(mut self, d: Dimension) -> Self {
        self.style.max_width = d;
        self
    }

    /// Set max-height.
    pub fn max_h(mut self, d: Dimension) -> Self {
        self.style.max_height = d;
        self
    }

    /// Set both width and height.
    pub fn size(mut self, w: Dimension, h: Dimension) -> Self {
        self.style.width = w;
        self.style.height = h;
        self
    }

    // -- Background / Visual --

    /// Set background color.
    pub fn bg(mut self, color: Color) -> Self {
        self.style.background = color;
        self
    }

    /// Set a linear gradient background.
    /// `angle` is in radians (0 = left→right, PI/2 = top→bottom).
    /// ラジアン回転を加える。正 = 画面上時計回り。
    ///
    /// 矩形は中心まわり (細長い rect を回転させて線として使う)、テキストは原点
    /// (左上) まわり — ピボットが違う。詳細は [`ElementStyle::rotation`]。
    pub fn rotation(mut self, radians: f32) -> Self {
        self.style.rotation = radians;
        self
    }

    pub fn gradient(mut self, from: Color, to: Color, angle: f32) -> Self {
        self.style.background = from;
        self.style.gradient_end = to;
        self.style.gradient_angle = angle;
        self
    }

    /// Set corner radius (all corners).
    pub fn rounded(mut self, d: Dimension) -> Self {
        let r = match d {
            Dimension::Px(v) => v,
            Dimension::Percent(v) => v, // percentage of min(width, height) resolved later
            Dimension::Auto => 0.0,
        };
        self.style.corner_radius = Corners::all(r);
        self
    }

    /// Set corner radius with a raw f32 (pixels).
    pub fn rounded_px(mut self, r: f32) -> Self {
        self.style.corner_radius = Corners::all(r);
        self
    }

    /// Set per-corner radius.
    pub fn corner_radius(mut self, corners: Corners<f32>) -> Self {
        self.style.corner_radius = corners;
        self
    }

    /// Set border.
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.style.border_width = width;
        self.style.border_color = color;
        self
    }

    /// Set border width only.
    pub fn border_width(mut self, width: f32) -> Self {
        self.style.border_width = width;
        self
    }

    /// Set border color only.
    pub fn border_color(mut self, color: Color) -> Self {
        self.style.border_color = color;
        self
    }

    /// Set box shadow.
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.style.shadow = Some(shadow);
        self
    }

    /// Quick shadow: `shadow_sm()` for a subtle elevation.
    pub fn shadow_sm(mut self, color: Color) -> Self {
        self.style.shadow = Some(BoxShadow {
            color,
            offset: Point::new(0.0, 2.0),
            blur: 4.0,
            spread: 0.0,
        });
        self
    }

    /// Quick shadow: `shadow_md()` for medium elevation.
    pub fn shadow_md(mut self, color: Color) -> Self {
        self.style.shadow = Some(BoxShadow {
            color,
            offset: Point::new(0.0, 4.0),
            blur: 8.0,
            spread: 0.0,
        });
        self
    }

    /// Glow effect (centered shadow with no offset).
    pub fn glow(mut self, color: Color, radius: f32) -> Self {
        self.style.shadow = Some(BoxShadow {
            color,
            offset: Point::ZERO,
            blur: radius,
            spread: 0.0,
        });
        self
    }

    /// Subtle glow (30% alpha, 8px blur).
    pub fn glow_sm(mut self, color: Color) -> Self {
        self.glow(color.with_alpha(0.3), 8.0)
    }

    /// Set opacity (0.0 = transparent, 1.0 = opaque).
    pub fn opacity(mut self, v: f32) -> Self {
        self.style.opacity = v;
        self
    }

    /// Set object-fit mode for Image elements.
    pub fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.style.object_fit = fit;
        self
    }

    // -- Padding --

    /// Set padding (all sides).
    pub fn p(mut self, d: Dimension) -> Self {
        self.style.padding = EdgeDimensions::all(d);
        self
    }

    /// Set padding with a raw f32 (pixels).
    pub fn p_px(mut self, v: f32) -> Self {
        self.style.padding = EdgeDimensions::all(Dimension::Px(v));
        self
    }

    /// Set horizontal padding.
    pub fn px_pad(mut self, d: Dimension) -> Self {
        self.style.padding.left = d;
        self.style.padding.right = d;
        self
    }

    /// Set vertical padding.
    pub fn py(mut self, d: Dimension) -> Self {
        self.style.padding.top = d;
        self.style.padding.bottom = d;
        self
    }

    /// Set padding-top.
    pub fn pt(mut self, d: Dimension) -> Self {
        self.style.padding.top = d;
        self
    }

    /// Set padding-right.
    pub fn pr(mut self, d: Dimension) -> Self {
        self.style.padding.right = d;
        self
    }

    /// Set padding-bottom.
    pub fn pb(mut self, d: Dimension) -> Self {
        self.style.padding.bottom = d;
        self
    }

    /// Set padding-left.
    pub fn pl(mut self, d: Dimension) -> Self {
        self.style.padding.left = d;
        self
    }

    // -- Margin --

    /// Set margin (all sides).
    pub fn m(mut self, d: Dimension) -> Self {
        self.style.margin = EdgeDimensions::all(d);
        self
    }

    /// Set margin with a raw f32 (pixels).
    pub fn m_px(mut self, v: f32) -> Self {
        self.style.margin = EdgeDimensions::all(Dimension::Px(v));
        self
    }

    /// Set horizontal margin.
    pub fn mx(mut self, d: Dimension) -> Self {
        self.style.margin.left = d;
        self.style.margin.right = d;
        self
    }

    /// Set vertical margin.
    pub fn my(mut self, d: Dimension) -> Self {
        self.style.margin.top = d;
        self.style.margin.bottom = d;
        self
    }

    /// Set margin-top.
    pub fn mt(mut self, d: Dimension) -> Self {
        self.style.margin.top = d;
        self
    }

    /// Set margin-right.
    pub fn mr(mut self, d: Dimension) -> Self {
        self.style.margin.right = d;
        self
    }

    /// Set margin-bottom.
    pub fn mb(mut self, d: Dimension) -> Self {
        self.style.margin.bottom = d;
        self
    }

    /// Set margin-left.
    pub fn ml(mut self, d: Dimension) -> Self {
        self.style.margin.left = d;
        self
    }

    // -- Flex layout --

    /// Set flex direction to column.
    pub fn flex_col(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Column;
        self
    }

    /// Set flex direction to row.
    pub fn flex_row(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Row;
        self
    }

    /// Set flex direction.
    pub fn flex_direction(mut self, dir: FlexDirection) -> Self {
        self.style.flex_direction = dir;
        self
    }

    /// Set flex wrap.
    pub fn flex_wrap(mut self, wrap: FlexWrap) -> Self {
        self.style.flex_wrap = wrap;
        self
    }

    /// Enable wrapping.
    pub fn wrap(mut self) -> Self {
        self.style.flex_wrap = FlexWrap::Wrap;
        self
    }

    /// Set flex grow.
    pub fn grow(mut self, v: f32) -> Self {
        self.style.flex_grow = v;
        self
    }

    /// Shortcut: `flex: 1 1 0%` (Tailwind `flex-1`).
    ///
    /// Sets `flex_grow: 1` AND `flex_basis: 0` so the item's share of the
    /// parent's main axis is computed from *free space*, not from its own
    /// content size. The basis-0 part is what makes
    /// `flex_1().overflow_scroll()` work without an explicit height: with
    /// the old `flex_basis: auto` behavior the scroll container's flex base
    /// size was its full content height, so it overflowed its slot (and
    /// squeezed its siblings) instead of scrolling. Use `.grow(1.0)` if you
    /// explicitly want grow-from-content behavior.
    pub fn flex_1(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self.style.flex_basis = Dimension::Px(0.0);
        self
    }

    /// Set the flex base size (CSS `flex-basis`). Default `Auto`.
    pub fn basis(mut self, d: Dimension) -> Self {
        self.style.flex_basis = d;
        self
    }

    /// Set flex shrink.
    pub fn shrink(mut self, v: f32) -> Self {
        self.style.flex_shrink = v;
        self
    }

    /// Set gap between children (pixels).
    pub fn gap(mut self, v: f32) -> Self {
        self.style.gap = v;
        self
    }

    /// Set align-items.
    pub fn align_items(mut self, a: AlignItems) -> Self {
        self.style.align_items = a;
        self
    }

    /// Shortcut: `align-items: center`.
    pub fn items_center(mut self) -> Self {
        self.style.align_items = AlignItems::Center;
        self
    }

    /// Shortcut: `align-items: start`.
    pub fn items_start(mut self) -> Self {
        self.style.align_items = AlignItems::Start;
        self
    }

    /// Shortcut: `align-items: end`.
    pub fn items_end(mut self) -> Self {
        self.style.align_items = AlignItems::End;
        self
    }

    /// Set justify-content.
    pub fn justify_content(mut self, j: JustifyContent) -> Self {
        self.style.justify_content = j;
        self
    }

    /// Shortcut: `justify-content: center`.
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = JustifyContent::Center;
        self
    }

    /// Shortcut: `justify-content: space-between`.
    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = JustifyContent::SpaceBetween;
        self
    }

    /// Shortcut: `justify-content: end`.
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = JustifyContent::End;
        self
    }

    /// Set overflow behavior.
    pub fn overflow(mut self, o: Overflow) -> Self {
        self.style.overflow = o;
        self
    }

    /// Set overflow to hidden.
    pub fn overflow_hidden(mut self) -> Self {
        self.style.overflow = Overflow::Hidden;
        self
    }

    /// Set overflow to scroll.
    pub fn overflow_scroll(mut self) -> Self {
        self.style.overflow = Overflow::Scroll;
        self
    }

    /// Set scroll offset (logical pixels). Only applies when overflow is Scroll.
    pub fn scroll_offset(mut self, x: f32, y: f32) -> Self {
        self.style.scroll_x = x;
        self.style.scroll_y = y;
        self
    }

    /// Draw a scrollbar thumb in `thumb` color at this scroll container's
    /// right edge whenever its content overflows vertically. Only meaningful
    /// together with `.overflow_scroll()`. Indicator only (not draggable);
    /// hidden while the content fits the viewport.
    pub fn scrollbar(mut self, thumb: Color) -> Self {
        self.style.scrollbar_thumb = Some(thumb);
        self
    }

    /// Set position type.
    pub fn position(mut self, p: Position) -> Self {
        self.style.position = p;
        self
    }

    /// Set position to absolute.
    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    /// Set absolute position (top, left) in pixels. Implies absolute positioning.
    pub fn pos(mut self, left: f32, top: f32) -> Self {
        self.style.position = Position::Absolute;
        self.style.inset_left = Dimension::Px(left);
        self.style.inset_top = Dimension::Px(top);
        self
    }

    /// Set a visual-only X translation in px. Layout (taffy) is unaffected;
    /// the element's render rect and hit region are simply shifted, so
    /// siblings stay in place. Designed for spring-driven hover anims.
    pub fn tx(mut self, x: f32) -> Self {
        self.style.translate_x = x;
        self
    }

    /// Set a visual-only Y translation in px. See [`Element::tx`].
    pub fn ty(mut self, y: f32) -> Self {
        self.style.translate_y = y;
        self
    }

    /// Set both X and Y visual translation in px. See [`Element::tx`].
    pub fn xlate(mut self, x: f32, y: f32) -> Self {
        self.style.translate_x = x;
        self.style.translate_y = y;
        self
    }

    // -- Text --

    /// Set text color.
    pub fn color(mut self, c: Color) -> Self {
        self.style.color = c;
        self
    }

    /// Set font size (pixels).
    pub fn font_size(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self
    }

    /// Set bold.
    pub fn bold(mut self) -> Self {
        self.style.bold = true;
        self
    }

    /// Set an explicit font weight (100–900), overriding `bold`. Common values:
    /// 300 light, 400 regular, 500 medium, 600 semibold, 700 bold. The face must
    /// provide the weight (system sans/mono do); otherwise cosmic-text picks the
    /// nearest available.
    pub fn font_weight(mut self, weight: u16) -> Self {
        self.style.font_weight = Some(weight);
        self
    }

    /// Add inter-glyph tracking in logical px. Positive spreads letters
    /// (spaced eyebrows / caps), negative tightens (large display headings).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.style.letter_spacing = px;
        self
    }

    /// Set line height as a multiple of the font size (CSS unitless
    /// `line-height`). `1.0` is snug; `1.5`–`1.6` is comfortable body copy.
    /// Unset (default) = 1.4.
    pub fn line_height(mut self, mult: f32) -> Self {
        self.style.line_height = Some(mult);
        self
    }

    /// Use monospace font.
    pub fn mono(mut self) -> Self {
        self.style.monospace = true;
        self
    }

    /// Shape this element's text in a specific named font family, overriding
    /// the generic (and any app-preferred) family. The face must be loaded in
    /// the renderer's font DB (system fonts are by default); an unknown name
    /// falls back through cosmic-text's normal fallback chain. Word-style font
    /// pickers use this to preview each row in its own face.
    pub fn font_family(mut self, name: impl Into<String>) -> Self {
        self.style.font_family = Some(name.into());
        self
    }

    /// Clamp wrapped text to at most `n` lines. The text wraps
    /// naturally within the element's computed width; if the
    /// wrapped output would exceed `n` lines, the last visible line
    /// gets truncated and `…` appended. Pass `0` to remove the
    /// clamp and restore unlimited wrap (the default).
    ///
    /// Most useful on `text(...)` elements inside a sized card
    /// where you want a preview that grows up to a known cap:
    ///
    /// ```ignore
    /// text(long_string).max_lines(2)  // up to 2 lines, then "…"
    /// ```
    pub fn max_lines(mut self, n: u32) -> Self {
        self.style.max_lines = if n == 0 { None } else { Some(n) };
        self
    }

    /// Paint background highlight rects behind the glyphs of the given byte
    /// ranges — Word-style find-in-page highlight. The ranges and colors are
    /// carried in [`HighlightSpec`]; the runtime resolves them to per-line
    /// rects using the same glyph hitboxes the text-selection system uses, so
    /// they track wrapping and CJK shaping exactly. Drawn below the text and
    /// above the element background. No-op on elements without text.
    ///
    /// ```ignore
    /// text(body).highlight(HighlightSpec {
    ///     ranges: vec![(4, 6), (20, 22)],
    ///     color: hit_bg,
    ///     current: Some(0),
    ///     current_color: active_bg,
    /// })
    /// ```
    ///
    /// **Calls accumulate.** One [`HighlightSpec`] paints one background color
    /// across its ranges (plus a second color for the single `current` range),
    /// which is all find-in-page needs. Text that carries two independent
    /// colorings at once — a new/old comparison where deletions are red and
    /// insertions are green, interleaved through one sentence — needs one spec
    /// per coloring:
    ///
    /// ```ignore
    /// text(body)
    ///     .highlight(HighlightSpec { ranges: deleted,  color: del_bg, ..Default::default() })
    ///     .highlight(HighlightSpec { ranges: inserted, color: add_bg, ..Default::default() })
    /// ```
    ///
    /// Specs paint in call order, so a later one covers an earlier one where
    /// their ranges overlap. Keeping the text a single element — rather than
    /// splitting it into one element per colored run — is the point: wrapping
    /// and CJK shaping stay intact because the runtime only lays rects behind
    /// glyphs it already positioned.
    pub fn highlight(mut self, spec: HighlightSpec) -> Self {
        self.style.highlight.push(spec);
        self
    }

    /// Mark byte ranges of this text element as clickable + hoverable in-body
    /// links. A pointer press inside a range dispatches `on_click(id)`; a
    /// `tooltip` shows on hover; the range is underlined in its `color`. The
    /// runtime resolves ranges to rects via the same glyph hitboxes as
    /// find-in-page highlight, so links track wrapping/CJK shaping. Used for
    /// 条文リンク in flowing 本文. No-op on elements without text.
    pub fn link_ranges(mut self, ranges: Vec<LinkRange>) -> Self {
        self.style.link_ranges = Some(ranges);
        self
    }

    // -- Button-specific --

    /// Set accent color (button background).
    pub fn accent(mut self, color: Color) -> Self {
        if let ElementKind::Button { ref mut accent, .. } = self.kind {
            *accent = Some(color);
        }
        // Also set as background for convenience
        self.style.background = color;
        self
    }

    // -- Identity --

    /// Set element ID for event dispatch (click/hover identification).
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the mouse cursor shown while hovering this element. Use
    /// `Cursor::Pointer` for buttons / clickable tiles; leave unset to
    /// inherit from ancestors / fall back to the platform default.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Opt this element and its entire subtree out of text selection —
    /// the CSS `user-select: none` equivalent.
    ///
    /// Put it on chrome (toolbars, sidebars, table headers, status bars) so
    /// that dragging inside a panel doesn't smear a selection highlight over
    /// every label. It inherits, so one call on the panel root covers the
    /// whole panel; prose stays selectable because you don't set it there.
    ///
    /// To kill selection app-wide instead, return `false` from
    /// `DeclarativeApp::text_selection_enabled`.
    pub fn no_select(mut self) -> Self {
        self.no_select = true;
        self
    }

    // -- Arc parameters (no-op on non-Arc elements) --

    /// Set the active fill fraction `[0, 1]` of an arc element.
    pub fn arc_value(mut self, v: f32) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.value = v.clamp(0.0, 1.0);
        }
        self
    }

    /// Set the donut band thickness (logical px) of an arc element.
    pub fn arc_thickness(mut self, t: f32) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.thickness = t.max(0.0);
        }
        self
    }

    /// Set both fill (active) and track (inactive) colors of an arc.
    pub fn arc_colors(mut self, fill: Color, track: Color) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.fill_color = fill;
            a.track_color = track;
        }
        self
    }

    /// Override the angular extent of an arc. `start` and `sweep` are
    /// in radians (screen-space convention, y grows downward). Default
    /// is `start = 0.75π` (bottom-left), `sweep = 1.5π` (270°
    /// tachometer style).
    pub fn arc_sweep(mut self, start: f32, sweep: f32) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.start_angle = start;
            a.sweep_angle = sweep.max(0.0);
        }
        self
    }

    // -- Polyline parameters (no-op on non-Polyline elements) --

    /// Set the vertices of a polyline. Points are logical px relative to
    /// the element's layout box origin.
    pub fn points(mut self, pts: impl Into<Vec<(f32, f32)>>) -> Self {
        if let ElementKind::Polyline(p) = &mut self.kind {
            p.points = pts.into();
        }
        self
    }

    /// Set the stroke width (logical px) of a polyline.
    pub fn stroke_width(mut self, w: f32) -> Self {
        if let ElementKind::Polyline(p) = &mut self.kind {
            p.width = w.max(0.0);
        }
        self
    }

    /// Set the stroke color of a polyline.
    pub fn stroke_color(mut self, c: Color) -> Self {
        if let ElementKind::Polyline(p) = &mut self.kind {
            p.color = c;
        }
        self
    }

    // -- Children --

    /// Add a single child.
    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple children.
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = Element>,
    {
        self.children.extend(children);
        self
    }

    // -- Events --

    /// Set click handler.
    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    /// Set hover handler.
    pub fn on_hover(mut self, handler: impl FnMut() + 'static) -> Self {
        self.on_hover = Some(Box::new(handler));
        self
    }

    /// Mark this element as focusable.
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    // -- Hover / Active state styles --

    /// Set hover style overrides via a closure.
    /// ```ignore
    /// div().bg(surface).hover(|s| s.bg(surface_hover).scale(1.02))
    /// ```
    pub fn hover(mut self, f: impl FnOnce(StateStyle) -> StateStyle) -> Self {
        self.hover_style = Some(f(StateStyle::default()));
        self
    }

    /// Set active (pressed) style overrides via a closure.
    /// ```ignore
    /// div().bg(surface).active(|s| s.bg(surface_active).scale(0.98))
    /// ```
    pub fn active(mut self, f: impl FnOnce(StateStyle) -> StateStyle) -> Self {
        self.active_style = Some(f(StateStyle::default()));
        self
    }

    /// Mark this element as a click target. Bundles the standard
    /// affordance — pointer cursor on hover, a small scale-up + caller-
    /// supplied bg shift on hover, a tiny scale-down on active (press).
    /// No transitions: snap by default. If you want the change to
    /// animate smoothly, chain `.spring_transition(k, c)` after.
    ///
    /// `hover_bg` is left to the caller because the right hover color
    /// is a design call — destructive actions usually want a red,
    /// neutral actions a subtle surface shift.
    ///
    /// ```ignore
    /// div().id("dismiss")
    ///     .w(Px(22.0)).h(Px(22.0))
    ///     .pressable(palette.surface_container_high)
    ///     .children([text("×")])
    /// ```
    pub fn pressable(self, hover_bg: Color) -> Self {
        self.cursor(Cursor::Pointer)
            .hover(move |s| s.scale(1.1).bg(hover_bg))
            .active(|s| s.scale(0.95))
    }

    // -- Transitions --

    /// Add a transition for a specific property.
    /// ```ignore
    /// div().transition(TransitionProperty::Background, TransitionKind::Easing {
    ///     duration: 0.3,
    ///     function: EasingFn::EaseOut,
    /// })
    /// ```
    pub fn transition(mut self, property: TransitionProperty, kind: TransitionKind) -> Self {
        self.transitions.push(Transition { property, kind });
        self
    }

    /// Shorthand: transition all properties with a spring.
    /// ```ignore
    /// div().spring_transition(200.0, 20.0)
    /// ```
    pub fn spring_transition(mut self, stiffness: f32, damping: f32) -> Self {
        self.transitions.push(Transition {
            property: TransitionProperty::All,
            kind: TransitionKind::Spring { stiffness, damping },
        });
        self
    }

    /// Shorthand: transition all properties with duration-based easing.
    /// ```ignore
    /// div().ease_transition(0.3, EasingFn::EaseOut)
    /// ```
    pub fn ease_transition(mut self, duration: f32, function: EasingFn) -> Self {
        self.transitions.push(Transition {
            property: TransitionProperty::All,
            kind: TransitionKind::Easing { duration, function },
        });
        self
    }

    // -- Overlay --

    /// Mark this element (and all its children) as overlay content.
    /// Overlay elements render on top of all base-layer content,
    /// preventing text bleed-through from lower elements.
    ///
    /// Also sets `position: Absolute` with `inset: 0,0` so the overlay is
    /// taken out of the parent flex flow (dropping a context menu / popup
    /// into a `flex_col` won't push the main content around) AND anchored
    /// to the parent's top-left corner by default. Callers that want
    /// custom positioning can chain `.pos(x, y)` afterwards.
    pub fn overlay(mut self) -> Self {
        self.overlay = true;
        self.style.position = Position::Absolute;
        self.style.inset_top = Dimension::Px(0.0);
        self.style.inset_left = Dimension::Px(0.0);
        self
    }

    // -- Tooltip --

    /// Set tooltip text shown on hover after a short delay.
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    // -- Drag & Drop --

    /// Mark this element as draggable with the given payload data.
    pub fn draggable(mut self, data: impl Into<String>) -> Self {
        self.drag_data = Some(data.into());
        self
    }

    /// Mark this element as a drop zone (accepts dragged items).
    pub fn droppable(mut self) -> Self {
        self.drop_zone = true;
        self
    }

    /// Enable presence animation (fade in/out when added/removed from tree).
    /// Requires `.id()` to be set for tracking.
    pub fn animate_presence(mut self) -> Self {
        self.animate_presence = true;
        self
    }
}

// ---------------------------------------------------------------------------
// StateStyle builder methods
// ---------------------------------------------------------------------------

impl StateStyle {
    pub fn bg(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadow = Some(Some(shadow));
        self
    }

    pub fn no_shadow(mut self) -> Self {
        self.shadow = Some(None);
        self
    }

    pub fn glow(mut self, color: Color, radius: f32) -> Self {
        self.shadow = Some(Some(BoxShadow {
            color,
            offset: Point::ZERO,
            blur: radius,
            spread: 0.0,
        }));
        self
    }

    pub fn glow_sm(mut self, color: Color) -> Self {
        self.glow(color.with_alpha(0.3), 8.0)
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn rounded(mut self, radii: Corners<f32>) -> Self {
        self.corner_radius = Some(radii);
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = Some(scale);
        self
    }

    pub fn translate_x(mut self, x: f32) -> Self {
        self.translate_x = Some(x);
        self
    }

    pub fn translate_y(mut self, y: f32) -> Self {
        self.translate_y = Some(y);
        self
    }

    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = Some(x);
        self.translate_y = Some(y);
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = Some(gap);
        self
    }

    pub fn w(mut self, d: Dimension) -> Self {
        self.width = Some(d);
        self
    }

    pub fn h(mut self, d: Dimension) -> Self {
        self.height = Some(d);
        self
    }

    pub fn p(mut self, d: Dimension) -> Self {
        self.padding = Some(EdgeDimensions::all(d));
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }
}
