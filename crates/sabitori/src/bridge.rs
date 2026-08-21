//! Bridge: converts RenderList (from declarative API) to GPU types.

use std::cell::RefCell;
use std::collections::HashMap;

use sabitori_core::build::{CaretPos, TextMeasure, TextShape};
use sabitori_core::element::{ImageData, ObjectFit, Typography};
use sabitori_core::render_list::{
    ImageDraw, PolylineDraw, RectDraw, RenderCommand, RenderList, RingDraw, TextDraw,
};
use sabitori_core::TextMetrics;
// `wgpu` comes via sabitori-gpu's re-export rather than a direct dependency, so
// there is exactly one wgpu version in the tree.
use sabitori_gpu::wgpu;
use sabitori_gpu::{ImageInstance, LineInstance, RectInstance, RingInstance};
use sabitori_text::{GlyphHit, GlyphInstance, TextRenderer};

/// Cache key for text measurement.
#[derive(Hash, Eq, PartialEq, Clone)]
struct MeasureKey {
    content: String,
    font_size_x100: u32,
    bold: bool,
    monospace: bool,
    /// Per-element family override (see `ElementStyle::font_family`) — part of
    /// the key, otherwise two elements with the same text but different faces
    /// share one (wrong) measurement.
    font_family: Option<String>,
    /// max_width bucketed to nearest 4 logical px. `u32::MAX` means "unconstrained".
    /// Bucketing prevents the cache from blowing up when widths shift by 1px each frame.
    max_width_bucket: u32,
    /// Extended typography folded into the key (weight, and the f32 tracking /
    /// line-height as raw bits) so a typographic change re-measures.
    weight: u16,
    letter_spacing_bits: u32,
    line_height_bits: Option<u32>,
    /// Line cap folded into the key so the clamped and unclamped heights of the
    /// same label don't collide (a `max_lines(1)` label must not reuse the full
    /// wrapped measurement cached for the same string elsewhere).
    max_lines: Option<u32>,
}

/// Persistent cache for text measurements. Lives across frames.
pub struct MeasureCache {
    entries: HashMap<MeasureKey, TextMetrics>,
}

impl MeasureCache {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    /// Number of cached measurements (e.g. for "clear on resize" tests
    /// and growth diagnostics in embedded hosts).
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Wraps a `&mut TextRenderer` + persistent cache for `TextMeasure`.
pub struct TextRendererMeasurer<'a> {
    renderer: RefCell<&'a mut TextRenderer>,
    cache: &'a RefCell<MeasureCache>,
}

impl<'a> TextRendererMeasurer<'a> {
    pub fn new(renderer: &'a mut TextRenderer, cache: &'a RefCell<MeasureCache>) -> Self {
        Self {
            renderer: RefCell::new(renderer),
            cache,
        }
    }
}

impl TextMeasure for TextRendererMeasurer<'_> {
    fn measure(
        &self,
        content: &str,
        font_size: f32,
        bold: bool,
        monospace: bool,
        font_family: Option<&str>,
        max_width: Option<f32>,
        max_lines: Option<u32>,
        typo: Typography,
    ) -> TextMetrics {
        let max_width_bucket = match max_width {
            Some(w) if w.is_finite() => ((w / 4.0).round() * 4.0) as u32,
            _ => u32::MAX,
        };
        let key = MeasureKey {
            content: content.to_string(),
            font_size_x100: (font_size * 100.0) as u32,
            bold,
            monospace,
            font_family: font_family.map(str::to_string),
            max_width_bucket,
            weight: typo.weight.unwrap_or(0),
            letter_spacing_bits: typo.letter_spacing.to_bits(),
            line_height_bits: typo.line_height.map(f32::to_bits),
            max_lines,
        };
        if let Some(&cached) = self.cache.borrow().entries.get(&key) {
            return cached;
        }
        // Cache the baseline alongside the box: it comes free from the same
        // shaping pass, and a caller that anchors on the baseline would
        // otherwise have to re-measure outside the cache.
        let metrics = self
            .renderer
            .borrow_mut()
            .measure_text(content, font_size, bold, monospace, font_family, max_width, max_lines, typo);
        self.cache.borrow_mut().entries.insert(key, metrics);
        metrics
    }

    // 折り返し系の 3 つは**キャッシュしない**。 キーに (offset / point / range)
    // まで入れると、 キャレットを 1 文字動かすたびに別のキーになるので、
    // キャッシュが当たらないまま際限なく太る。 呼ばれるのはフォーカス中の
    // テキスト欄 1 個ぶんで、 1 フレームに数回なので実測でも問題にならない。

    fn caret_pos(&self, content: &str, byte_offset: usize, shape: TextShape<'_>) -> CaretPos {
        self.renderer
            .borrow_mut()
            .shaper
            .caret_pos(content, byte_offset, shape)
    }

    fn offset_at(&self, content: &str, point: (f32, f32), shape: TextShape<'_>) -> usize {
        self.renderer.borrow_mut().shaper.offset_at(content, point, shape)
    }

    fn range_rects(
        &self,
        content: &str,
        range: (usize, usize),
        shape: TextShape<'_>,
    ) -> Vec<sabitori_core::Rect> {
        self.renderer.borrow_mut().shaper.range_rects(content, range, shape)
    }
}

/// Convert a [`RingDraw`] to a [`RingInstance`] for GPU rendering. The
/// ring renderer expects the inner radius separately, derived in
/// `build.rs` from the element's layout box + arc thickness.
pub fn ring_to_instance(d: &RingDraw) -> RingInstance {
    // Colors stay un-premultiplied; the shader applies alpha during
    // SDF coverage, matching the rect pipeline's convention.
    RingInstance {
        center_radii: [d.center.x, d.center.y, d.outer_radius, d.inner_radius],
        arc_params: [d.start_angle, d.sweep_angle, d.value, 0.0],
        fill_color: d.fill_color.to_array(),
        track_color: d.track_color.to_array(),
        clip_rect: [0.0; 4],
    }
}

/// Expand a [`PolylineDraw`] into one [`LineInstance`] per segment
/// (N points → N-1 segments). Round caps keep the joints seamless.
pub fn polyline_to_instances(d: &PolylineDraw) -> Vec<LineInstance> {
    let half = (d.width * 0.5).max(0.0);
    let color = d.color.to_array();
    d.points
        .windows(2)
        .map(|w| LineInstance {
            endpoints: [w[0].x, w[0].y, w[1].x, w[1].y],
            params: [half, 0.75, 0.0, 0.0],
            color,
            clip_rect: [0.0; 4],
        })
        .collect()
}

/// True if a polyline is fully outside `clip`, tested via the
/// axis-aligned bounding box of all its points.
fn polyline_clipped(clip: &sabitori_core::Rect, points: &[sabitori_core::Point]) -> bool {
    let mut minx = f32::MAX;
    let mut miny = f32::MAX;
    let mut maxx = f32::MIN;
    let mut maxy = f32::MIN;
    for p in points {
        minx = minx.min(p.x);
        miny = miny.min(p.y);
        maxx = maxx.max(p.x);
        maxy = maxy.max(p.y);
    }
    let bbox = sabitori_core::Rect::new(minx, miny, maxx - minx, maxy - miny);
    is_clipped(clip, &bbox)
}

/// Convert a RectDraw to a RectInstance for GPU rendering.
pub fn rect_to_instance(d: &RectDraw) -> RectInstance {
    let mut fill = d.fill_color;
    fill.a *= d.opacity;

    RectInstance {
        rect: [d.rect.origin.x, d.rect.origin.y, d.rect.size.width, d.rect.size.height],
        corner_radii: d.corner_radii.to_array(),
        fill_color: fill.to_array(),
        border_color: d.border_color.to_array(),
        border_width: d.border_width,
        gradient_angle: d.gradient_angle,
        rotation: d.rotation,
        _pad0: 0.0,
        shadow_color: d.shadow_color.to_array(),
        shadow_offset: [d.shadow_offset.x, d.shadow_offset.y],
        shadow_params: [d.shadow_blur, d.shadow_spread],
        gradient_end_color: d.gradient_end_color.to_array(),
        clip_rect: [0.0; 4],
    }
}

/// Convert a TextDraw to GlyphInstances via the TextRenderer.
pub fn text_to_glyphs(d: &TextDraw, tr: &mut TextRenderer) -> Vec<GlyphInstance> {
    let max_width = if d.max_width > 0.0 && d.max_width < f32::MAX {
        Some(d.max_width)
    } else {
        None
    };
    let mut glyphs = tr.prepare_text_styled(
        &d.content, d.position.x, d.position.y,
        d.font_size, d.color, max_width,
        d.bold, d.monospace, d.font_family.as_deref(), d.max_lines,
        d.typo,
    );
    // Rotation is applied *after* shaping so the shaping cache stays
    // angle-independent — the same string at a new angle is still a cache hit.
    sabitori_text::rotate_glyphs(&mut glyphs, (d.position.x, d.position.y), d.rotation);
    glyphs
}

/// Check if a rect is fully outside a clip rect.
///
/// A degenerate clip (zero width or height — e.g. the running intersection
/// of two non-overlapping nested clips) clips EVERYTHING. This must be
/// culled here on the CPU: the GPU instances use `w==0||h==0` in
/// `clip_rect` as the "unclipped" sentinel, so writing a degenerate clip
/// to an instance would DISABLE clipping for it and leak it over the
/// whole screen.
fn is_clipped(clip: &sabitori_core::Rect, item: &sabitori_core::Rect) -> bool {
    if clip.size.width <= 0.0 || clip.size.height <= 0.0 {
        return true;
    }
    item.origin.x + item.size.width < clip.origin.x
        || item.origin.x > clip.origin.x + clip.size.width
        || item.origin.y + item.size.height < clip.origin.y
        || item.origin.y > clip.origin.y + clip.size.height
}

fn clip_to_array(r: &sabitori_core::Rect) -> [f32; 4] {
    [r.origin.x, r.origin.y, r.size.width, r.size.height]
}

/// Bounding rect used to cull a `TextDraw` against the clip stack.
///
/// The height MUST be the text's real laid-out height: `TextDraw.max_height`
/// is set in `build.rs` from the taffy node rect (measured via `TextMeasure`,
/// wrapping included), so it is the actual multi-line height — not merely a
/// "layout constraint". Using a fixed `font_size * 1.5` here (as the old code
/// did) culls a TALL paragraph the instant its *top* scrolls ~1.5 lines above
/// the viewport, even though its lower lines are still on screen → text
/// "disappears" while scrolling. We floor at 1.5 lines so a degenerate/zero
/// `max_height` never over-culls a genuinely visible single line.
fn text_cull_rect(d: &TextDraw) -> sabitori_core::Rect {
    let h = d.max_height.max(d.font_size * 1.5);
    let w = d.max_width.max(1.0);
    if d.rotation == 0.0 {
        return sabitori_core::Rect::new(d.position.x, d.position.y, w, h);
    }
    // Rotated text swings around its origin, so the unrotated box no longer
    // contains it — culling against that box would drop text that has swung
    // into view. Take the AABB of the four rotated corners instead (same
    // pivot and sign as `rotate_glyphs`).
    let (sin, cos) = d.rotation.sin_cos();
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (dx, dy) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
        let x = d.position.x + dx * cos - dy * sin;
        let y = d.position.y + dx * sin + dy * cos;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    sabitori_core::Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Intersect two clip rects. Returns a degenerate rect (zero size) when
/// they do not overlap.
fn intersect_clip(a: &sabitori_core::Rect, b: &sabitori_core::Rect) -> sabitori_core::Rect {
    let left = a.origin.x.max(b.origin.x);
    let top = a.origin.y.max(b.origin.y);
    let right = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let bottom = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    let w = (right - left).max(0.0);
    let h = (bottom - top).max(0.0);
    sabitori_core::Rect::new(left, top, w, h)
}

/// Push `incoming` onto `clip_stack`, intersecting with the previous top
/// so the stack always holds the running clip-intersection as its top
/// element. Mirrors how a real graphics scissor stack composes.
fn push_intersected_clip(
    clip_stack: &mut Vec<sabitori_core::Rect>,
    incoming: sabitori_core::Rect,
) {
    let merged = match clip_stack.last() {
        Some(parent) => intersect_clip(parent, &incoming),
        None => incoming,
    };
    clip_stack.push(merged);
}

/// 1 つの TextDraw のレイアウト結果 (selection hit-test 用)。 reading-order index
/// + 元の content + 各 glyph の hitbox (advance space)。 declarative runtime が
/// mouse 座標から文字位置を解決するのに使う。
#[derive(Clone, Debug)]
pub struct TextHitLayout {
    /// `RenderList::texts()` を enumerate した時の 0-based index。 selection state
    /// の anchor / head はこの index + byte offset でテキスト要素を一意に指す。
    pub text_idx: usize,
    /// 元の表示テキスト (truncate 後の `…` 付きを含む可能性あり)。 clipboard
    /// extract で substring に使う。
    pub content: String,
    /// 各 glyph の hitbox。 行は `line_index` でグループ化されてる。
    pub hits: Vec<GlyphHit>,
    /// 描画時に効いていた scissor clip (overflow_scroll / overflow_hidden の交差)。
    /// 「画面外」 判定や hit-test 内外の判定に使う。 None = clip 無し。
    pub clip_rect: Option<sabitori_core::Rect>,
    /// この TextDraw に付いていたハイライト指定 (byte 範囲 + 色) を描画順に。
    /// runtime が `hits` を使って背景 rect に解決する。 空 = ハイライト無し。
    /// 複数持てるのは、 1 つの spec が塗れる色が「全範囲 1 色 + current 1 範囲だけ
    /// 別色」に限られるため。 新旧対照の見え消しのように、 1 つの文の中で赤地と
    /// 緑地が交互に来る塗り分けは spec を 2 つ重ねて表現する。
    pub highlight: Vec<sabitori_core::HighlightSpec>,
    /// この TextDraw に付いていた in-body リンク範囲 (byte 範囲 + id + tooltip)。
    /// runtime が `hits` を使って click/hover の hit-test と下線 rect に解決する。
    pub link_ranges: Option<Vec<sabitori_core::LinkRange>>,
    /// `user-select: none` 相当 (`Element::no_select` の継承結果、 または button の
    /// label)。 `true` の layout は selection の hit-test / 塗り / clipboard 抽出の
    /// 全部から外れる。 highlight と link は selection と別系統なので効き続ける。
    pub no_select: bool,
}

/// Convert an entire RenderList into GPU-ready data, applying clip rects.
pub fn render_list_to_gpu(
    list: &RenderList,
    tr: &mut TextRenderer,
) -> (Vec<RectInstance>, Vec<GlyphInstance>) {
    let (rects, glyphs, _rings, _lines) = render_list_to_gpu_with_rings(list, tr);
    (rects, glyphs)
}

/// `render_list_to_gpu_with_rings` の selection 拡張版。 通常の rects/glyphs/rings
/// に加えて、 各 TextDraw の `TextHitLayout` を reading-order で返す。 declarative
/// runtime が selection の hit-test / 描画 / clipboard 抽出に使う。
pub fn render_list_to_gpu_with_hits(
    list: &RenderList,
    tr: &mut TextRenderer,
) -> (Vec<RectInstance>, Vec<GlyphInstance>, Vec<RingInstance>, Vec<LineInstance>, Vec<TextHitLayout>) {
    // Self-heal a full glyph atlas before shaping this frame's text: if it
    // overflowed last frame (dropped glyphs → blank/missing text), flush it now
    // so the visible glyph set re-rasterizes fresh. See `maybe_recover_atlas`.
    tr.maybe_recover_atlas();
    let mut rects = Vec::with_capacity(list.rect_count());
    let mut glyphs = Vec::new();
    let mut rings: Vec<RingInstance> = Vec::new();
    let mut lines: Vec<LineInstance> = Vec::new();
    let mut text_layouts: Vec<TextHitLayout> = Vec::new();
    let mut clip_stack: Vec<sabitori_core::Rect> = Vec::new();
    let mut text_idx: usize = 0;

    for cmd in &list.commands {
        match cmd {
            RenderCommand::PushClip(clip_rect) => {
                push_intersected_clip(&mut clip_stack, *clip_rect);
            }
            RenderCommand::PopClip => {
                clip_stack.pop();
            }
            RenderCommand::Rect(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if is_clipped(&c, &d.rect) { continue; }
                }
                let mut inst = rect_to_instance(d);
                if let Some(c) = clip {
                    inst.clip_rect = clip_to_array(&c);
                }
                rects.push(inst);
            }
            RenderCommand::Ring(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    let r = d.outer_radius;
                    let bbox = sabitori_core::Rect::new(
                        d.center.x - r,
                        d.center.y - r,
                        2.0 * r,
                        2.0 * r,
                    );
                    if is_clipped(&c, &bbox) { continue; }
                }
                let mut inst = ring_to_instance(d);
                if let Some(c) = clip {
                    inst.clip_rect = clip_to_array(&c);
                }
                rings.push(inst);
            }
            RenderCommand::Text(d) => {
                let clip = clip_stack.last().copied();
                let visible = match clip {
                    Some(c) => !is_clipped(&c, &text_cull_rect(d)),
                    None => true,
                };
                // text_idx は 「reading-order の通し番号」 なので、 描画時に
                // clip で skip されたテキストも index を進める。 そうしないと
                // scroll out → scroll in で index がズレて selection state が
                // 別 element を指してしまう。
                let cur_idx = text_idx;
                text_idx += 1;
                if !visible {
                    continue;
                }
                let max_width = if d.max_width > 0.0 && d.max_width < f32::MAX {
                    Some(d.max_width)
                } else {
                    None
                };
                let (mut produced, hits) = tr.prepare_text_with_hits(
                    &d.content, d.position.x, d.position.y,
                    d.font_size, d.color, max_width,
                    d.bold, d.monospace, d.font_family.as_deref(), d.max_lines,
                    d.typo,
                );
                // Glyphs turn; `hits` deliberately do NOT. Selection, caret and
                // link hit-testing all read the axis-aligned boxes, so on
                // rotated text they describe where the run *would* sit unrotated.
                // Rotating them would need every consumer to do an oriented-box
                // test — out of scope while nothing rotates interactive text.
                sabitori_text::rotate_glyphs(
                    &mut produced,
                    (d.position.x, d.position.y),
                    d.rotation,
                );
                if let Some(c) = clip {
                    let arr = clip_to_array(&c);
                    for g in produced.iter_mut() {
                        g.clip_rect = arr;
                    }
                }
                glyphs.extend(produced);
                text_layouts.push(TextHitLayout {
                    text_idx: cur_idx,
                    content: d.content.clone(),
                    hits,
                    clip_rect: clip,
                    highlight: d.highlight.clone(),
                    link_ranges: d.link_ranges.clone(),
                    no_select: d.no_select,
                });
            }
            RenderCommand::Polyline(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if polyline_clipped(&c, &d.points) { continue; }
                }
                let mut insts = polyline_to_instances(d);
                if let Some(c) = clip {
                    let arr = clip_to_array(&c);
                    for i in insts.iter_mut() {
                        i.clip_rect = arr;
                    }
                }
                lines.extend(insts);
            }
            RenderCommand::Image(_) => {}
        }
    }

    (rects, glyphs, rings, lines, text_layouts)
}

/// Like [`render_list_to_gpu`] but also returns ring instances. The
/// declarative event loop passes the ring vec to
/// [`sabitori_gpu::RingRenderer::render_rings`] inside the
/// `extra_draw` callback.
pub fn render_list_to_gpu_with_rings(
    list: &RenderList,
    tr: &mut TextRenderer,
) -> (Vec<RectInstance>, Vec<GlyphInstance>, Vec<RingInstance>, Vec<LineInstance>) {
    // Self-heal a full glyph atlas before shaping this frame's text (see the
    // `_with_hits` sibling and `maybe_recover_atlas`).
    tr.maybe_recover_atlas();
    let mut rects = Vec::with_capacity(list.rect_count());
    let mut glyphs = Vec::new();
    let mut rings: Vec<RingInstance> = Vec::new();
    let mut lines: Vec<LineInstance> = Vec::new();
    let mut clip_stack: Vec<sabitori_core::Rect> = Vec::new();

    for cmd in &list.commands {
        match cmd {
            RenderCommand::PushClip(clip_rect) => {
                push_intersected_clip(&mut clip_stack, *clip_rect);
            }
            RenderCommand::PopClip => {
                clip_stack.pop();
            }
            RenderCommand::Rect(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if is_clipped(&c, &d.rect) { continue; }
                }
                let mut inst = rect_to_instance(d);
                if let Some(c) = clip {
                    inst.clip_rect = clip_to_array(&c);
                }
                rects.push(inst);
            }
            RenderCommand::Ring(d) => {
                // Bound the arc by its outer radius for clip-test.
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    let r = d.outer_radius;
                    let bbox = sabitori_core::Rect::new(
                        d.center.x - r,
                        d.center.y - r,
                        2.0 * r,
                        2.0 * r,
                    );
                    if is_clipped(&c, &bbox) { continue; }
                }
                let mut inst = ring_to_instance(d);
                if let Some(c) = clip {
                    inst.clip_rect = clip_to_array(&c);
                }
                rings.push(inst);
            }
            RenderCommand::Text(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if is_clipped(&c, &text_cull_rect(d)) { continue; }
                }
                let mut produced = text_to_glyphs(d, tr);
                if let Some(c) = clip {
                    let arr = clip_to_array(&c);
                    for g in produced.iter_mut() {
                        g.clip_rect = arr;
                    }
                }
                glyphs.extend(produced);
            }
            RenderCommand::Polyline(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if polyline_clipped(&c, &d.points) { continue; }
                }
                let mut insts = polyline_to_instances(d);
                if let Some(c) = clip {
                    let arr = clip_to_array(&c);
                    for i in insts.iter_mut() {
                        i.clip_rect = arr;
                    }
                }
                lines.extend(insts);
            }
            RenderCommand::Image(_) => {
                // Image rendering handled separately by the image pipeline
            }
        }
    }

    (rects, glyphs, rings, lines)
}

/// Convert base and overlay RenderLists into separate GPU-ready data.
///
/// Returns `(base_rects, base_glyphs, overlay_rects, overlay_glyphs)`.
/// The caller must draw in order: base_rects -> base_glyphs -> overlay_rects -> overlay_glyphs
/// to ensure overlay content renders on top of all base content.
pub fn render_list_to_gpu_layered(
    base: &RenderList,
    overlay: &RenderList,
    tr: &mut TextRenderer,
) -> (Vec<RectInstance>, Vec<GlyphInstance>, Vec<RectInstance>, Vec<GlyphInstance>) {
    let (base_rects, base_glyphs) = render_list_to_gpu(base, tr);
    let (overlay_rects, overlay_glyphs) = render_list_to_gpu(overlay, tr);
    (base_rects, base_glyphs, overlay_rects, overlay_glyphs)
}

// ---------------------------------------------------------------------------
// Image batching
// ---------------------------------------------------------------------------

/// A batch of image instances sharing the same texture.
pub struct ImageBatch {
    pub key: String,
    pub data: ImageData,
    pub instances: Vec<ImageInstance>,
}

/// Compute UV rect for object-fit behavior.
fn compute_object_fit_uv(
    image_w: u32, image_h: u32,
    rect_w: f32, rect_h: f32,
    fit: ObjectFit,
) -> [f32; 4] {
    match fit {
        ObjectFit::Fill => [0.0, 0.0, 1.0, 1.0],
        ObjectFit::Cover => {
            let img_aspect = image_w as f32 / image_h as f32;
            let rect_aspect = rect_w / rect_h;
            if img_aspect > rect_aspect {
                let visible_w = rect_aspect / img_aspect;
                let offset_u = (1.0 - visible_w) * 0.5;
                [offset_u, 0.0, visible_w, 1.0]
            } else {
                let visible_h = img_aspect / rect_aspect;
                let offset_v = (1.0 - visible_h) * 0.5;
                [0.0, offset_v, 1.0, visible_h]
            }
        }
        ObjectFit::Contain => [0.0, 0.0, 1.0, 1.0],
    }
}

/// Extract image batches from a RenderList, grouped by key.
///
/// Images are clipped against the active scroll/overflow clip rect: when a
/// clip rect partially overlaps an image, the image's destination rect is
/// shrunk to the visible intersection and its UV rect is proportionally
/// sub-sampled so only the visible portion renders. Without this, images
/// inside scroll containers bleed out of their bounds (e.g. onto a fixed
/// toolbar or into neighbouring regions).
pub fn extract_image_batches(list: &RenderList) -> Vec<ImageBatch> {
    let mut batches: Vec<ImageBatch> = Vec::new();
    let mut clip_stack: Vec<sabitori_core::Rect> = Vec::new();

    for cmd in &list.commands {
        match cmd {
            RenderCommand::PushClip(clip_rect) => {
                push_intersected_clip(&mut clip_stack, *clip_rect);
            }
            RenderCommand::PopClip => {
                clip_stack.pop();
            }
            RenderCommand::Image(d) => {
                let base_uv = compute_object_fit_uv(
                    d.data.width, d.data.height,
                    d.rect.size.width, d.rect.size.height,
                    d.object_fit,
                );

                // Fully-outside → skip; partial → shrink rect + sub-sample
                // UV so the image is clipped at scroll container edges and
                // does not bleed over neighbouring regions (toolbars, headers).
                let (rect, uv) = if let Some(clip) = clip_stack.last() {
                    if is_clipped(clip, &d.rect) { continue; }
                    clip_image(&d.rect, base_uv, clip)
                } else {
                    (
                        [d.rect.origin.x, d.rect.origin.y, d.rect.size.width, d.rect.size.height],
                        base_uv,
                    )
                };

                let inst = ImageInstance {
                    rect,
                    uv_rect: uv,
                    corner_radii: d.corner_radii.to_array(),
                    params: [d.opacity, 0.0, 0.0, 0.0],
                };

                if let Some(batch) = batches.iter_mut().find(|b| b.key == d.key) {
                    batch.instances.push(inst);
                } else {
                    batches.push(ImageBatch {
                        key: d.key.clone(),
                        data: d.data.clone(),
                        instances: vec![inst],
                    });
                }
            }
            _ => {}
        }
    }

    batches
}

/// Everything one UI pass paints on top of the rect layer, in painter order.
///
/// Built once per render list so the draw site does not have to remember which
/// extraction call yields which kind — see [`draw_ui_layer`].
#[derive(Default)]
pub struct UiDrawLists {
    pub images: Vec<ImageBatch>,
    pub rings: Vec<RingInstance>,
    pub lines: Vec<LineInstance>,
    pub glyphs: Vec<GlyphInstance>,
}

impl UiDrawLists {
    /// Whether this layer has nothing to draw.
    ///
    /// The renderer takes rects itself and hands everything else back through
    /// a phase callback, so "no rects" and "nothing to draw" are different
    /// questions. A layer holding only an image, or only text, has zero rects
    /// (an undecorated `div` emits none) but is very much not empty — see
    /// [`GpuRenderer::render_layered`](sabitori_gpu::GpuRenderer::render_layered).
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
            && self.rings.is_empty()
            && self.lines.is_empty()
            && self.glyphs.is_empty()
    }

    /// Extract every non-rect draw kind from one render list, returning the
    /// rects alongside (they are drawn by the renderer itself, before the pass
    /// callback runs).
    pub fn extract(list: &RenderList, tr: &mut TextRenderer) -> (Vec<RectInstance>, Self) {
        let (rects, glyphs, rings, lines) = render_list_to_gpu_with_rings(list, tr);
        (
            rects,
            Self { images: extract_image_batches(list), rings, lines, glyphs },
        )
    }

    /// [`UiDrawLists::extract`] の selection 拡張版。 テキスト選択 / find-in-page を
    /// 扱う呼び出し側は、 次フレームのヒットテスト用に `TextHitLayout` も要る。
    pub fn extract_with_hits(
        list: &RenderList,
        tr: &mut TextRenderer,
    ) -> (Vec<RectInstance>, Self, Vec<TextHitLayout>) {
        let (rects, glyphs, rings, lines, layouts) = render_list_to_gpu_with_hits(list, tr);
        (
            rects,
            Self { images: extract_image_batches(list), rings, lines, glyphs },
            layouts,
        )
    }
}

/// The renderers one UI pass draws with. Image/ring/line are optional because
/// callers hold them behind `Option` and `take()` them out of `self` to split
/// the borrow against the render closure; text is always present.
pub struct UiRenderers<'a> {
    pub images: Option<&'a mut sabitori_gpu::ImageRenderer>,
    pub rings: Option<&'a mut sabitori_gpu::RingRenderer>,
    pub lines: Option<&'a mut sabitori_gpu::LineRenderer>,
    pub text: &'a mut TextRenderer,
}

/// Draw one UI layer inside an open render pass: images → rings → polylines →
/// glyphs, so text always lands on top.
///
/// **この関数が描画順とパイプラインの網羅を持つ唯一の場所。** 以前は declarative と
/// scene_app が同じ並びを各所で手書きしていて、 scene_app 側だけ image / ring / line
/// が丸ごと抜けており、 該当要素が警告も無く消えていた (#72)。 描画種別を足すときは
/// ここ 1 箇所だけを触ること。
///
/// 2 つの順序上の注意:
/// - テクスチャのアップロードは `queue.write_texture` 経由で、 pass ではなく次の
///   `submit` に対して順序付けされる。 よって pass の encode 中に呼んでも、
///   pass の実行より前に必ず反映される。
/// - 種別ごとに 1 pass 1 回しか描画呼び出しをしない。 `queue.write_buffer` は
///   submit ごとに 1 度しか効かないので、 バッチ単位でループして描くと共有の
///   instance buffer を後続の書き込みが潰してしまう。
pub fn draw_ui_layer(
    r: &mut UiRenderers<'_>,
    lists: &UiDrawLists,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    globals_bg: &wgpu::BindGroup,
) {
    if let Some(img_r) = r.images.as_deref_mut() {
        if !lists.images.is_empty() {
            for b in &lists.images {
                img_r.ensure_texture(
                    device, queue, &b.key,
                    &b.data.rgba, b.data.width, b.data.height,
                );
            }
            img_r.render_many(
                device,
                queue,
                lists.images.iter().map(|b| (b.key.as_str(), b.instances.as_slice())),
                pass,
                globals_bg,
            );
        }
    }
    if let Some(ring_r) = r.rings.as_deref_mut() {
        ring_r.render_rings(device, queue, &lists.rings, pass, globals_bg);
    }
    if let Some(line_r) = r.lines.as_deref_mut() {
        line_r.render_lines(device, queue, &lists.lines, pass, globals_bg);
    }
    r.text.render_glyphs(device, queue, &lists.glyphs, pass, globals_bg);
}

/// Intersect image's destination rect with the clip rect and proportionally
/// sub-sample the UV so the visible portion of the image maps to the visible
/// portion of the destination.
fn clip_image(
    img_rect: &sabitori_core::Rect,
    base_uv: [f32; 4],
    clip: &sabitori_core::Rect,
) -> ([f32; 4], [f32; 4]) {
    let x = img_rect.origin.x;
    let y = img_rect.origin.y;
    let w = img_rect.size.width.max(0.0001);
    let h = img_rect.size.height.max(0.0001);

    let left = x.max(clip.origin.x);
    let top = y.max(clip.origin.y);
    let right = (x + w).min(clip.origin.x + clip.size.width);
    let bottom = (y + h).min(clip.origin.y + clip.size.height);

    if right <= left || bottom <= top {
        // Fully clipped; caller already guards against this but be defensive.
        return (
            [left, top, 0.0, 0.0],
            [base_uv[0], base_uv[1], 0.0, 0.0],
        );
    }

    let u_off = (left - x) / w;
    let v_off = (top - y) / h;
    let u_frac = (right - left) / w;
    let v_frac = (bottom - top) / h;

    let uv = [
        base_uv[0] + u_off * base_uv[2],
        base_uv[1] + v_off * base_uv[3],
        u_frac * base_uv[2],
        v_frac * base_uv[3],
    ];
    (
        [left, top, right - left, bottom - top],
        uv,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::Rect;

    // 退化 clip (幅または高さ 0) は「全てを clip する」。GPU インスタンスの
    // clip_rect は w==0||h==0 を「clip 無し」センチネルに使っているため、
    // 退化 clip を素通しすると clip が無効化されて中身が画面全体に漏れる。
    // ここで必ず CPU 側で cull すること。
    #[test]
    fn degenerate_clip_clips_everything() {
        let degenerate = Rect::new(0.0, 500.0, 100.0, 0.0);
        // Item straddling the degenerate clip's anchor line — the old
        // "fully outside?" test let this through and the GPU sentinel then
        // rendered it UNCLIPPED.
        let straddling = Rect::new(0.0, 490.0, 50.0, 20.0);
        assert!(is_clipped(&degenerate, &straddling));
        // Items anywhere else are culled too.
        assert!(is_clipped(&degenerate, &Rect::new(10.0, 510.0, 50.0, 20.0)));
        let zero_w = Rect::new(50.0, 0.0, 0.0, 100.0);
        assert!(is_clipped(&zero_w, &Rect::new(40.0, 10.0, 20.0, 20.0)));
    }

    // 重ならない入れ子 clip の交差は退化 rect になる。スタックの top が
    // 退化していたら、その中の要素は 1 つも GPU に流れないこと。
    #[test]
    fn disjoint_nested_clips_cull_contents() {
        let mut stack: Vec<Rect> = Vec::new();
        push_intersected_clip(&mut stack, Rect::new(0.0, 0.0, 100.0, 100.0));
        // Inner clip container laid out entirely below the outer clip
        // (e.g. scrolled out of view).
        push_intersected_clip(&mut stack, Rect::new(0.0, 500.0, 100.0, 100.0));
        let top = *stack.last().unwrap();
        assert!(
            top.size.width <= 0.0 || top.size.height <= 0.0,
            "disjoint clips must intersect to a degenerate rect, got {top:?}"
        );
        // Anything inside the inner container must be culled.
        assert!(is_clipped(&top, &Rect::new(10.0, 510.0, 50.0, 20.0)));
        assert!(is_clipped(&top, &Rect::new(0.0, 495.0, 100.0, 10.0)));
    }

    // 正常な clip では従来どおり: 部分重なりは通し (GPU 側で discard)、
    // 完全に外側だけ cull。
    #[test]
    fn normal_clip_culls_only_fully_outside() {
        let clip = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(!is_clipped(&clip, &Rect::new(50.0, 90.0, 20.0, 40.0))); // partial
        assert!(!is_clipped(&clip, &Rect::new(10.0, 10.0, 20.0, 20.0))); // inside
        assert!(is_clipped(&clip, &Rect::new(10.0, 200.0, 20.0, 20.0))); // below
        assert!(is_clipped(&clip, &Rect::new(200.0, 10.0, 20.0, 20.0))); // right
    }

    fn rotated_text(rotation: f32) -> TextDraw {
        TextDraw {
            content: "室名".into(),
            position: sabitori_core::Point::new(100.0, 100.0),
            max_width: 200.0,
            // font_size * 1.5 (= 24) より大きくしておく。text_cull_rect は
            // 両者の max を取るので、ここが小さいとテストが暗黙に 24 を見る。
            max_height: 40.0,
            font_size: 16.0,
            color: sabitori_core::Color::TRANSPARENT,
            bold: false,
            monospace: false,
            font_family: None,
            max_lines: None,
            typo: Typography::default(),
            highlight: Vec::new(),
            link_ranges: None,
            rotation,
            no_select: false,
        }
    }

    // 回転テキストの cull は「回した 4 隅の AABB」で見ること。無回転の箱で
    // 判定すると、原点まわりに振れて画面内に入ってきた注記が消える（DXF の
    // 90 度注記が clip 境界付近で丸ごと欠ける）。
    #[test]
    fn rotated_text_cull_rect_covers_the_swung_box() {
        // 無回転: 従来どおり position 起点の素の箱。
        let flat = text_cull_rect(&rotated_text(0.0));
        assert_eq!((flat.origin.x, flat.origin.y), (100.0, 100.0));
        assert_eq!((flat.size.width, flat.size.height), (200.0, 40.0));

        // +90 度（画面時計回り, Y 下向き）: 幅 200 が下へ伸び、高さ 40 が左へ出る。
        let turned = text_cull_rect(&rotated_text(std::f32::consts::FRAC_PI_2));
        assert!((turned.origin.x - 60.0).abs() < 1e-3, "{turned:?}");
        assert!((turned.origin.y - 100.0).abs() < 1e-3, "{turned:?}");
        assert!((turned.size.width - 40.0).abs() < 1e-3, "{turned:?}");
        assert!((turned.size.height - 200.0).abs() < 1e-3, "{turned:?}");

        // 回転後にだけ重なる clip は cull されない（無回転の箱なら外れている）。
        let clip = Rect::new(0.0, 250.0, 95.0, 100.0);
        assert!(is_clipped(&clip, &flat));
        assert!(!is_clipped(&clip, &turned));
    }

    // 斜め 45 度は 4 隅すべてが効く（対角が AABB を決める）。どれか 1 隅でも
    // 落とすと箱が痩せて誤 cull する。
    #[test]
    fn diagonal_rotation_uses_all_four_corners() {
        let r = text_cull_rect(&rotated_text(std::f32::consts::FRAC_PI_4));
        let (w, h) = (200.0_f32, 40.0_f32);
        let s = std::f32::consts::FRAC_1_SQRT_2;
        // x: 左端は (0,h) 隅の -h*s、右端は (w,0) 隅の +w*s。
        assert!((r.origin.x - (100.0 - h * s)).abs() < 1e-3, "{r:?}");
        assert!((r.size.width - (w + h) * s).abs() < 1e-3, "{r:?}");
        // y: 上端は position のまま、下端は (w,h) 隅。
        assert!((r.origin.y - 100.0).abs() < 1e-3, "{r:?}");
        assert!((r.size.height - (w + h) * s).abs() < 1e-3, "{r:?}");
    }

    // 背の高い（複数行）テキスト段落は、先頭が viewport 上端を超えても
    // 下端が viewport 内にある限り cull されないこと。旧コードは高さを
    // font_size*1.5 決め打ちで見ていたため、先頭が ~1.5 行分スクロール
    // アウトした瞬間に段落まるごと消えた（= スクロールで文字が消えるバグ）。
    #[test]
    fn tall_text_straddling_clip_top_is_not_culled() {
        let clip = Rect::new(0.0, 0.0, 300.0, 400.0); // viewport
        let make = |pos_y: f32, max_h: f32| TextDraw {
            content: "あ".repeat(400),
            position: sabitori_core::Point::new(0.0, pos_y),
            max_width: 280.0,
            max_height: max_h,
            font_size: 16.0,
            color: sabitori_core::Color::TRANSPARENT,
            bold: false,
            monospace: false,
            font_family: None,
            max_lines: None,
            typo: Typography::default(),
            highlight: Vec::new(),
            link_ranges: None,
            rotation: 0.0,
            no_select: false,
        };
        // top を -200 までスクロール、高さ 300 → bottom=100 は viewport 内 → 描く。
        // 旧 font_size*1.5(=24px) 近似だと top=-200 の rect は viewport 外 → 誤 cull。
        assert!(!is_clipped(&clip, &text_cull_rect(&make(-200.0, 300.0))));
        // 完全に上へ抜けた（bottom も上）ものは従来どおり cull。
        assert!(is_clipped(&clip, &text_cull_rect(&make(-400.0, 300.0))));
        // 単一行（max_height 小）は 1.5 行 floor 挙動を維持。
        assert!(is_clipped(&clip, &text_cull_rect(&make(-40.0, 18.0))));
    }
}

#[cfg(test)]
mod fullpath_verify {
    use super::*;
    use sabitori_core::{build_tree, Color, Px};
    use sabitori_core::element::polyline;
    use sabitori_gpu::LineRenderer;

    fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;
        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor { label: Some("fullpath"), ..Default::default() },
            None,
        ))
        .ok()
    }

    #[test]
    fn polyline_element_renders_through_build_and_bridge() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("skip: no GPU");
            return;
        };
        let (w, h): (u32, u32) = (512, 256);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;

        // --- THE REAL PATH: polyline() element → build_tree → bridge ---
        let mut pts: Vec<(f32, f32)> = Vec::new();
        for i in 0..=90 {
            let t = i as f32 / 90.0;
            let x = 24.0 + t * (w as f32 - 48.0);
            let y = h as f32 * 0.5 - 92.0 * (t * std::f32::consts::PI * 3.0).sin();
            pts.push((x, y));
        }
        let root = polyline()
            .w(Px(w as f32))
            .h(Px(h as f32))
            .points(pts)
            .stroke_width(3.0)
            .stroke_color(Color::from_hex("#4ddbe8"));
        let build = build_tree(&root, w as f32, h as f32);

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let mut tr = TextRenderer::new(&device, format, &globals_layout);
        let (_rects, _glyphs, _rings, lines) =
            render_list_to_gpu_with_rings(&build.render_list, &mut tr);
        eprintln!("bridge produced {} line instances from polyline()", lines.len());
        assert!(!lines.is_empty(), "polyline() produced no line instances!");

        // --- render exactly those instances ---
        let globals: [f32; 4] = [w as f32, h as f32, 1.0, 0.0];
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&globals_buf, 0, bytemuck::cast_slice(&globals));
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals_bg"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }],
        });
        let mut lr = LineRenderer::new(&device, format, &globals_layout);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.09, g: 0.10, b: 0.15, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            lr.render_lines(&device, &queue, &lines, &mut pass, &globals_bg);
        }
        let bpr = w * 4;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (bpr * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        let out = std::env::temp_dir().join("polyline_fullpath.png");
        image::save_buffer(&out, &data[..], w, h, image::ExtendedColorType::Rgba8).unwrap();
        eprintln!("WROTE {}", out.display());
    }
}

#[cfg(test)]
mod overlay_emptiness_tests {
    //! `UiDrawLists::is_empty` は、レンダラに「この層を開くべきか」を伝えるための
    //! 判定。矩形はレンダラ自身が描き、それ以外は callback 越しなので、
    //! 「矩形が 0」と「描く物が無い」は別の問いになる ([#44]).
    //!
    //! [#44]: https://github.com/Mutafika/sabitori/issues/44

    use super::*;

    fn a_pixel() -> sabitori_core::ImageData {
        sabitori_core::ImageData::new(vec![255, 0, 0, 255], 1, 1)
    }

    #[test]
    fn nothing_at_all_is_empty() {
        assert!(UiDrawLists::default().is_empty());
    }

    /// ドラッグゴーストの形 — 画像だけ。矩形は 0 でも、層は空ではない。
    #[test]
    fn an_image_alone_is_not_empty() {
        let lists = UiDrawLists {
            images: vec![ImageBatch {
                key: "ghost".into(),
                data: a_pixel(),
                instances: Vec::new(),
            }],
            ..Default::default()
        };
        assert!(!lists.is_empty());
    }

    /// 文字だけの層も同じ。
    #[test]
    fn glyphs_alone_are_not_empty() {
        let lists = UiDrawLists {
            glyphs: vec![bytemuck::Zeroable::zeroed()],
            ..Default::default()
        };
        assert!(!lists.is_empty());
    }

    /// リング / 線だけの層も落としてはいけない。
    #[test]
    fn rings_and_lines_alone_are_not_empty() {
        let rings = UiDrawLists {
            rings: vec![bytemuck::Zeroable::zeroed()],
            ..Default::default()
        };
        let lines = UiDrawLists {
            lines: vec![bytemuck::Zeroable::zeroed()],
            ..Default::default()
        };
        assert!(!rings.is_empty(), "リングだけの層");
        assert!(!lines.is_empty(), "線だけの層");
    }
}
