//! Render command output types.
//!
//! [`build_tree`](crate::build::build_tree) converts an element tree into a flat
//! list of [`RenderCommand`]s that the GPU renderer can consume.

use crate::{Color, Corners, Point, Rect};
use crate::element::{ImageData, ObjectFit};

/// A single render command produced by the build step.
#[derive(Clone, Debug)]
pub enum RenderCommand {
    /// Draw a filled/bordered rounded rectangle.
    Rect(RectDraw),
    /// Draw a text string at a position.
    Text(TextDraw),
    /// Draw an image.
    Image(ImageDraw),
    /// Draw an arc / ring segment (donut sector with active fill +
    /// inactive track sub-arcs).
    Ring(RingDraw),
    /// Draw a polyline — an open sequence of connected line segments.
    Polyline(PolylineDraw),
    /// Push a scissor clip rectangle. All subsequent draws are clipped to this rect.
    PushClip(Rect),
    /// Pop the most recent clip rectangle.
    PopClip,
}

/// Draw a polyline: an open sequence of connected line segments,
/// SDF-rasterized as capsules with round joints (see `line.wgsl`). Drawn
/// as N-1 segment instances.
#[derive(Clone, Debug)]
pub struct PolylineDraw {
    /// Vertices in absolute logical-pixel coords.
    pub points: Vec<Point>,
    /// Stroke width (logical px).
    pub width: f32,
    /// Stroke color.
    pub color: Color,
}

/// Draw an arc / ring segment. The renderer rasterizes a single donut
/// sector via SDF: the angular range `[start_angle, start_angle +
/// sweep]` defines the full arc, of which the first
/// `value * sweep` is filled with `fill_color` and the remainder with
/// `track_color`.
#[derive(Clone, Copy, Debug)]
pub struct RingDraw {
    /// Center of the ring in absolute logical-pixel coords.
    pub center: Point,
    /// Outer radius of the donut band (logical pixels).
    pub outer_radius: f32,
    /// Inner radius — the "hole" of the donut. Must be ≥ 0 and <
    /// `outer_radius`.
    pub inner_radius: f32,
    /// Angle (radians) at which the arc begins, measured in standard
    /// math convention (+x axis = 0, +y axis = π/2 in *screen-space*
    /// where y grows downward, so `0.75π` is bottom-left, `1.5π` is
    /// straight down, etc.).
    pub start_angle: f32,
    /// Total sweep (radians) covered by the full track. Positive.
    pub sweep_angle: f32,
    /// Active fill fraction, clamped to `[0, 1]`. The first
    /// `value * sweep_angle` of the arc renders in `fill_color`; the
    /// rest in `track_color`.
    pub value: f32,
    /// Active fill color.
    pub fill_color: Color,
    /// Inactive track color.
    pub track_color: Color,
}

impl Default for RingDraw {
    fn default() -> Self {
        Self {
            center: Point::ZERO,
            outer_radius: 0.0,
            inner_radius: 0.0,
            start_angle: 0.0,
            sweep_angle: std::f32::consts::TAU,
            value: 0.0,
            fill_color: Color::TRANSPARENT,
            track_color: Color::TRANSPARENT,
        }
    }
}

/// Draw a rounded rectangle.
#[derive(Clone, Copy, Debug)]
pub struct RectDraw {
    /// Absolute position and size in logical pixels.
    pub rect: Rect,
    /// Corner radii (top-left, top-right, bottom-right, bottom-left).
    pub corner_radii: Corners<f32>,
    /// Fill color.
    pub fill_color: Color,
    /// Border color.
    pub border_color: Color,
    /// Border width in logical pixels.
    pub border_width: f32,
    /// Shadow color (transparent = no shadow).
    pub shadow_color: Color,
    /// Shadow offset.
    pub shadow_offset: Point,
    /// Shadow blur radius.
    pub shadow_blur: f32,
    /// Shadow spread.
    pub shadow_spread: f32,
    /// Opacity (0.0 = invisible, 1.0 = fully opaque).
    pub opacity: f32,
    /// Gradient angle in radians (0.0 = no gradient).
    pub gradient_angle: f32,
    /// Gradient end color. Fill color is the start.
    pub gradient_end_color: Color,
    /// 回転角 (ラジアン)。中心まわりに回転。線描画用。
    pub rotation: f32,
}

impl Default for RectDraw {
    fn default() -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            corner_radii: Corners::all(0.0),
            fill_color: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            shadow_color: Color::TRANSPARENT,
            shadow_offset: Point::ZERO,
            shadow_blur: 0.0,
            shadow_spread: 0.0,
            opacity: 1.0,
            gradient_angle: 0.0,
            gradient_end_color: Color::TRANSPARENT,
            rotation: 0.0,
        }
    }
}

/// Draw text at a position.
#[derive(Clone, Debug)]
pub struct TextDraw {
    /// The text content.
    pub content: String,
    /// Top-left position in logical pixels.
    pub position: Point,
    /// Maximum width for line wrapping.
    pub max_width: f32,
    /// Maximum height for clipping.
    pub max_height: f32,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Text color.
    pub color: Color,
    /// Whether the text is bold.
    pub bold: bool,
    /// Whether to use a monospace font.
    pub monospace: bool,
    /// Specific font family overriding the generic resolution (see
    /// `ElementStyle::font_family`). `None` = monospace/sans-serif generics.
    pub font_family: Option<String>,
    /// Optional max visible lines. `None` = unlimited (Excel-cell
    /// style wrap), `Some(n)` = clamp to `n` lines with trailing
    /// `…` when more content would follow.
    pub max_lines: Option<u32>,
    /// Extended typographic controls (weight / letter-spacing / line-height).
    pub typo: crate::element::Typography,
    /// Per-range background highlights, in paint order. Empty = none.
    /// The runtime resolves the byte ranges to per-line rects from this text's
    /// glyph hitboxes and paints them below the glyphs.
    pub highlight: Vec<crate::element::HighlightSpec>,
    /// Optional clickable/hoverable byte ranges (in-body links). The runtime
    /// resolves them to glyph hitboxes for click/hover dispatch + underline.
    pub link_ranges: Option<Vec<crate::element::LinkRange>>,
    /// 回転角 (ラジアン)。`position` (= レイアウト後の左上) まわりに回転する。
    /// 符号は `RectDraw::rotation` と同じで、Y 下向きの画面座標なので正 = 画面上
    /// 時計回り。既定 `0.0` は無回転。
    ///
    /// ピボットが `RectDraw` (矩形の**中心**) と違う点に注意。CAD の注記は挿入点
    /// まわりに回るのが仕様なのでテキストは原点ピボットにしてあるが、その結果
    /// 「背景付きの要素を回すと箱とラベルがずれる」。
    ///
    /// 回転は shaping の**後**に掛かる。折返し (`max_width`) も
    /// `max_lines` の切り詰めも回転前の水平レイアウトで決まる。
    ///
    /// クリップ (scissor) とヒットテストは軸並行のまま — 回転テキストでは
    /// どちらも近似になる。詳細は `sabitori_text::rotate_glyphs`。
    pub rotation: f32,
    /// `user-select: none` 相当。`true` なら runtime の selection がこのテキストを
    /// 一切掴まない (anchor/head にならない・選択背景を塗らない・clipboard 抽出でも
    /// 飛ばす)。`Element::no_select` が subtree に継承された結果と、button の label
    /// (= コントロールのラベルであって本文ではない) がここに立つ。
    pub no_select: bool,
}

/// Draw an image at a position.
#[derive(Clone, Debug)]
pub struct ImageDraw {
    /// Unique key for texture caching.
    pub key: String,
    /// Raw RGBA8 pixel data.
    pub data: ImageData,
    /// Destination rectangle in logical pixels.
    pub rect: Rect,
    /// Corner radii for rounded clipping.
    pub corner_radii: Corners<f32>,
    /// Opacity (0.0 = invisible, 1.0 = opaque).
    pub opacity: f32,
    /// How the image fills the rect.
    pub object_fit: ObjectFit,
}

/// A complete render list: the output of building an element tree.
#[derive(Clone, Debug, Default)]
pub struct RenderList {
    pub commands: Vec<RenderCommand>,
}

impl RenderList {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    /// Iterate over rect draw commands only.
    pub fn rects(&self) -> impl Iterator<Item = &RectDraw> {
        self.commands.iter().filter_map(|cmd| {
            if let RenderCommand::Rect(r) = cmd {
                Some(r)
            } else {
                None
            }
        })
    }

    /// Iterate over text draw commands only.
    pub fn texts(&self) -> impl Iterator<Item = &TextDraw> {
        self.commands.iter().filter_map(|cmd| {
            if let RenderCommand::Text(t) = cmd {
                Some(t)
            } else {
                None
            }
        })
    }

    /// Number of rect draw commands.
    pub fn rect_count(&self) -> usize {
        self.commands.iter().filter(|c| matches!(c, RenderCommand::Rect(_))).count()
    }

    /// Number of text draw commands.
    pub fn text_count(&self) -> usize {
        self.commands.iter().filter(|c| matches!(c, RenderCommand::Text(_))).count()
    }
}
