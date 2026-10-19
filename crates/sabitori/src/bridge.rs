//! Bridge: converts RenderList (from declarative API) to GPU types.

use std::cell::RefCell;
use std::collections::HashMap;

use sabitori_core::build::{CaretPos, TextMeasure, TextShape};
use sabitori_core::element::{GradientKind, ImageData, ObjectFit, Typography};
use sabitori_core::render_list::{
    GooDraw, ImageDraw, PolylineDraw, RectDraw, RenderCommand, RenderList, RingDraw, TextDraw,
};
use sabitori_core::TextMetrics;
// `wgpu` comes via sabitori-gpu's re-export rather than a direct dependency, so
// there is exactly one wgpu version in the tree.
use sabitori_gpu::wgpu;
use sabitori_gpu::{GooInstance, GooSlot, ImageInstance, LineInstance, RectInstance, RingInstance};
use sabitori_text::{GlyphHit, GlyphInstance, TextRenderer};

/// `TextDraw` の前景色スパンを、テキスト側の型へ移す (#78)。
///
/// `sabitori-core` は `sabitori-text` を知らない (依存の向きが逆) ので、
/// 同じ形の型が両側にある。写すのはここ 1 箇所。
fn color_spans(d: &TextDraw) -> Vec<sabitori_text::ColorSpan> {
    match &d.color_spans {
        None => Vec::new(),
        Some(spans) => spans
            .iter()
            .map(|s| sabitori_text::ColorSpan {
                start: s.start,
                end: s.end,
                color: s.color.to_array(),
            })
            .collect(),
    }
}

/// この文字に **hitbox を作る必要があるか** ([#80])。
///
/// `hits` を読むのは 3 つだけ — テキスト選択、ハイライトの敷き、in-body
/// リンクの当たり判定。`no_select` が付いていてハイライトもリンクも無い文字は、
/// 作った瞬間に捨てる `Vec` になる。端末や表のように「選べない文字が数百個」
/// 並ぶ画面では、これが要素ごとに 1 確保として効く。
///
/// **読む側を増やしたらここも増やすこと。** 増やし忘れると、選択やリンクが
/// 「特定の文字でだけ効かない」という掴みにくい形で壊れる。
///
/// [#80]: https://github.com/Mutafika/sabitori/issues/80
pub(crate) fn needs_hit_layout(d: &TextDraw) -> bool {
    !d.no_select || !d.highlight.is_empty() || d.link_ranges.is_some()
}

/// 計測キャッシュの鍵。
///
/// **文字列を持たない。** 以前は `content: String` と `font_family: Option<String>`
/// を毎回作っていたので、**引くためだけに 1〜2 個確保**していた。taffy は
/// 1 ノードにつき measure を複数回呼ぶので、その回数だけ増える
/// ([#80](https://github.com/Mutafika/sabitori/issues/80))。
///
/// 代わりに 64bit のハッシュを鍵にする — シェーピング側の `run_cache_key` が
/// 同じ形。**鍵にする材料を 1 つ落とすと、違うものが同じ測定値を共有する**
/// (太字だけ違う 2 つのラベルが同じ幅になる) ので、追加したら
/// `measure_key_tests` にも 1 行足すこと。
fn measure_key(
    content: &str,
    font_size: f32,
    bold: bool,
    monospace: bool,
    font_family: Option<&str>,
    max_width_bucket: u32,
    max_lines: Option<u32>,
    typo: Typography,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    ((font_size * 100.0) as u32).hash(&mut h);
    bold.hash(&mut h);
    monospace.hash(&mut h);
    match font_family {
        Some(f) => {
            1u8.hash(&mut h);
            f.hash(&mut h);
        }
        None => 0u8.hash(&mut h),
    }
    max_width_bucket.hash(&mut h);
    typo.weight.unwrap_or(0).hash(&mut h);
    typo.letter_spacing.to_bits().hash(&mut h);
    typo.line_height.map(f32::to_bits).hash(&mut h);
    typo.italic.hash(&mut h);
    (typo.align as u8).hash(&mut h);
    max_lines.hash(&mut h);
    h.finish()
}

/// Persistent cache for text measurements. Lives across frames.
pub struct MeasureCache {
    entries: HashMap<u64, TextMetrics>,
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
        let key = measure_key(
            content,
            font_size,
            bold,
            monospace,
            font_family,
            max_width_bucket,
            max_lines,
            typo,
        );
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
    // Colors stay un-premultiplied (straight); the shader multiplies rgb by
    // alpha × SDF coverage, matching every other UI pipeline.
    RingInstance {
        center_radii: [d.center.x, d.center.y, d.outer_radius, d.inner_radius],
        arc_params: [d.start_angle, d.sweep_angle, d.value, 0.0],
        fill_color: d.fill_color.to_array(),
        track_color: d.track_color.to_array(),
        clip_rect: [0.0; 4],
    }
}

/// Convert a [`GooDraw`] to a [`GooInstance`]. Colors stay straight, like
/// every other UI pipeline.
pub fn goo_to_instance(d: &GooDraw) -> GooInstance {
    let rect = |r: &sabitori_core::Rect| [r.origin.x, r.origin.y, r.size.width, r.size.height];
    GooInstance {
        rect_a: rect(&d.a),
        rect_b: rect(&d.b),
        params: [d.radius_a, d.radius_b, d.smooth, 0.0],
        color: d.color.to_array(),
        clip_rect: [0.0; 4],
        color_end: d.color_end.to_array(),
        border_color: d.border_color.to_array(),
        shadow_color: d.shadow_color.to_array(),
        surface: [d.border_width, d.shadow_blur, d.shadow_offset.x, d.shadow_offset.y],
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
    // `d.opacity` はここで掛けない。`build.rs` が色ごとに (`apply_opacity`)
    // 掛け終えている — ここで fill だけにもう一度掛けると、塗りは op² で、
    // 枠と影は op で薄くなり、フェード中の箱の中身と縁がずれる。
    let fill = d.fill_color;

    RectInstance {
        rect: [d.rect.origin.x, d.rect.origin.y, d.rect.size.width, d.rect.size.height],
        corner_radii: d.corner_radii.to_array(),
        fill_color: fill.to_array(),
        border_color: d.border_color.to_array(),
        border_width: d.border_width,
        gradient_angle: d.gradient_angle,
        rotation: d.rotation,
        _pad0: 0.0,
        paint: match d.gradient_kind {
            GradientKind::None => [0.0; 4],
            GradientKind::Linear => [1.0, 0.0, 0.0, 0.0],
            GradientKind::Radial { cx, cy } => [2.0, cx, cy, 0.0],
        },
        // 種は f32 で正確に持てる 24 bit に畳む。
        grain: [d.grain.amount, (d.grain.seed & 0xff_ffff) as f32, 0.0, 0.0],
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
    // 前景色のスパン (#78) はここも通す。**text を GPU に渡す道は 2 本ある**
    // (`text_to_glyphs` と `render_list_to_gpu_with_hits`) ので、片方だけ直すと
    // 「画面外描画では色が付くのに窓では単色」のような分かれ方をする。
    let mut glyphs = tr.prepare_text_spans(
        &d.content, d.position.x, d.position.y,
        d.font_size, d.color, &color_spans(d), max_width,
        d.bold, d.monospace, d.font_family.as_deref(), d.max_lines,
        d.typo,
    );
    // Rotation is applied *after* shaping so the shaping cache stays
    // angle-independent — the same string at a new angle is still a cache hit.
    sabitori_text::rotate_glyphs(&mut glyphs, (d.position.x, d.position.y), d.rotation);
    sabitori_text::fill_backdrop(&mut glyphs, d.backdrop);
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
    ///
    /// `Arc<str>` なのは、描画結果から選択の突き合わせまで**同じ文字列を
    /// 参照カウントで持ち回す**ため (#80)。
    pub content: std::sync::Arc<str>,
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
    /// id の付いたいちばん近い祖先の id ([`TextDraw::owner`])。選択範囲をアプリへ
    /// 渡すときに使う (#107)。
    pub owner: Option<std::sync::Arc<str>>,
}

/// 格子の字形 ([#102](https://github.com/Mutafika/sabitori/issues/102))。
/// 2 つの取り出し口 (当たり判定あり・なし) が同じものを出すよう、ここに 1 つだけ置く。
fn cell_grid_to_glyphs(
    d: &sabitori_core::render_list::CellGridDraw,
    tr: &mut TextRenderer,
    clip: Option<sabitori_core::Rect>,
) -> Vec<GlyphInstance> {
    let mut produced = tr.prepare_cell_grid(
        &d.grid,
        d.visible_rows(clip),
        d.origin.x,
        d.origin.y,
        d.cell_w,
        d.cell_h,
        d.font_size,
        d.opacity,
        d.font_family.as_deref(),
        d.cache_key,
    );
    sabitori_text::fill_backdrop(&mut produced, d.backdrop);
    if let Some(c) = clip {
        let arr = clip_to_array(&c);
        for g in produced.iter_mut() {
            g.clip_rect = arr;
        }
    }
    produced
}

/// Convert an entire RenderList into GPU-ready data, applying clip rects.
#[allow(deprecated)]
#[deprecated(since = "0.41.0", note = "種類ごとにまとめて描くと、重なった下の要素の文字や画像が上の箱に透ける (#126)。木の順に描く `bridge::OrderedFrame` を使う")]
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
#[deprecated(since = "0.41.0", note = "種類ごとにまとめて描くと、重なった下の要素の文字や画像が上の箱に透ける (#126)。木の順に描く `bridge::OrderedFrame` を使う")]
pub fn render_list_to_gpu_with_hits(
    list: &RenderList,
    tr: &mut TextRenderer,
) -> (Vec<RectInstance>, Vec<GlyphInstance>, Vec<RingInstance>, Vec<LineInstance>, Vec<TextHitLayout>) {
    render_list_to_gpu_with_hits_impl(list, tr, &mut Vec::new())
}

/// [`render_list_to_gpu_with_hits`] that also collects goo, positioned in the returned rect
/// sequence (see [`GooSlot`]).
fn render_list_to_gpu_with_hits_impl(
    list: &RenderList,
    tr: &mut TextRenderer,
    goo: &mut Vec<GooSlot>,
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

    let mut mask_stack = Vec::new();
    for cmd in &list.commands {
        if skip_mask(cmd, &mut mask_stack) {
            continue;
        }
        match cmd {
            RenderCommand::PushClip(clip_rect) => {
                push_intersected_clip(&mut clip_stack, *clip_rect);
            }
            RenderCommand::PopClip => {
                clip_stack.pop();
            }
            // 種類ごとにまとめて描く古い経路は、グループを回さずその場に描く。
            RenderCommand::PushGroup(_) | RenderCommand::PushMask | RenderCommand::PopGroup => {}
            // パスの塗りは木の順に描く経路だけ。
            RenderCommand::PathFill(_) => {}
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
                let (mut produced, hits) = if needs_hit_layout(d) {
                    let (g, h) = tr.prepare_text_with_hits_spans(
                        &d.content, d.position.x, d.position.y,
                        d.font_size, d.color, &color_spans(d), max_width,
                        d.bold, d.monospace, d.font_family.as_deref(), d.max_lines,
                        d.typo,
                    );
                    (g, Some(h))
                } else {
                    let g = tr.prepare_text_spans(
                        &d.content, d.position.x, d.position.y,
                        d.font_size, d.color, &color_spans(d), max_width,
                        d.bold, d.monospace, d.font_family.as_deref(), d.max_lines,
                        d.typo,
                    );
                    (g, None)
                };
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
                sabitori_text::fill_backdrop(&mut produced, d.backdrop);
                if let Some(c) = clip {
                    let arr = clip_to_array(&c);
                    for g in produced.iter_mut() {
                        g.clip_rect = arr;
                    }
                }
                glyphs.extend(produced);
                if let Some(hits) = hits {
                    text_layouts.push(TextHitLayout {
                        text_idx: cur_idx,
                        content: d.content.clone(),
                        hits,
                        clip_rect: clip,
                        highlight: d.highlight.clone(),
                        link_ranges: d.link_ranges.clone(),
                        no_select: d.no_select,
                        owner: d.owner.clone(),
                    });
                }
            }
            RenderCommand::Goo(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if is_clipped(&c, &d.bounds()) { continue; }
                }
                let mut inst = goo_to_instance(d);
                if let Some(c) = clip {
                    inst.clip_rect = clip_to_array(&c);
                }
                // Drawn between rects, in tree order — it is a background.
                goo.push(GooSlot { before_rect: rects.len() as u32, instance: inst });
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
            RenderCommand::CellGrid(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if is_clipped(&c, &d.rect()) { continue; }
                }
                glyphs.extend(cell_grid_to_glyphs(d, tr, clip));
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
#[deprecated(since = "0.41.0", note = "種類ごとにまとめて描くと、重なった下の要素の文字や画像が上の箱に透ける (#126)。木の順に描く `bridge::OrderedFrame` を使う")]
pub fn render_list_to_gpu_with_rings(
    list: &RenderList,
    tr: &mut TextRenderer,
) -> (Vec<RectInstance>, Vec<GlyphInstance>, Vec<RingInstance>, Vec<LineInstance>) {
    render_list_to_gpu_with_rings_impl(list, tr, &mut Vec::new())
}

/// [`render_list_to_gpu_with_rings`] that also collects goo, positioned in the returned rect
/// sequence (see [`GooSlot`]).
fn render_list_to_gpu_with_rings_impl(
    list: &RenderList,
    tr: &mut TextRenderer,
    goo: &mut Vec<GooSlot>,
) -> (Vec<RectInstance>, Vec<GlyphInstance>, Vec<RingInstance>, Vec<LineInstance>) {
    // Self-heal a full glyph atlas before shaping this frame's text (see the
    // `_with_hits` sibling and `maybe_recover_atlas`).
    tr.maybe_recover_atlas();
    let mut rects = Vec::with_capacity(list.rect_count());
    let mut glyphs = Vec::new();
    let mut rings: Vec<RingInstance> = Vec::new();
    let mut lines: Vec<LineInstance> = Vec::new();
    let mut clip_stack: Vec<sabitori_core::Rect> = Vec::new();

    let mut mask_stack = Vec::new();
    for cmd in &list.commands {
        if skip_mask(cmd, &mut mask_stack) {
            continue;
        }
        match cmd {
            RenderCommand::PushClip(clip_rect) => {
                push_intersected_clip(&mut clip_stack, *clip_rect);
            }
            RenderCommand::PopClip => {
                clip_stack.pop();
            }
            // 種類ごとにまとめて描く古い経路は、グループを回さずその場に描く。
            RenderCommand::PushGroup(_) | RenderCommand::PushMask | RenderCommand::PopGroup => {}
            // パスの塗りは木の順に描く経路だけ。
            RenderCommand::PathFill(_) => {}
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
            RenderCommand::Goo(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if is_clipped(&c, &d.bounds()) { continue; }
                }
                let mut inst = goo_to_instance(d);
                if let Some(c) = clip {
                    inst.clip_rect = clip_to_array(&c);
                }
                // Drawn between rects, in tree order — it is a background.
                goo.push(GooSlot { before_rect: rects.len() as u32, instance: inst });
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
            RenderCommand::CellGrid(d) => {
                let clip = clip_stack.last().copied();
                if let Some(c) = clip {
                    if is_clipped(&c, &d.rect()) { continue; }
                }
                glyphs.extend(cell_grid_to_glyphs(d, tr, clip));
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
#[allow(deprecated)]
#[deprecated(since = "0.41.0", note = "種類ごとにまとめて描くと、重なった下の要素の文字や画像が上の箱に透ける (#126)。木の順に描く `bridge::OrderedFrame` を使う")]
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

/// 画像 1 枚の instance。切り抜きの外なら `None`、一部だけなら見える所だけに縮めて
/// UV も合わせて削る (スクロールの縁で隣の領域へはみ出さない)。
fn image_instance(d: &ImageDraw, clip: Option<&sabitori_core::Rect>) -> Option<ImageInstance> {
    let base_uv = compute_object_fit_uv(
        d.data.width, d.data.height,
        d.rect.size.width, d.rect.size.height,
        d.object_fit,
    );
    let (rect, uv) = match clip {
        Some(clip) => {
            if is_clipped(clip, &d.rect) {
                return None;
            }
            clip_image(&d.rect, base_uv, clip)
        }
        None => (
            [d.rect.origin.x, d.rect.origin.y, d.rect.size.width, d.rect.size.height],
            base_uv,
        ),
    };
    Some(ImageInstance {
        rect,
        uv_rect: uv,
        corner_radii: d.corner_radii.to_array(),
        params: [d.opacity, 0.0, 0.0, 0.0],
    })
}

/// Extract image batches from a RenderList, grouped by key.
///
/// Images are clipped against the active scroll/overflow clip rect: when a
/// clip rect partially overlaps an image, the image's destination rect is
/// shrunk to the visible intersection and its UV rect is proportionally
/// sub-sampled so only the visible portion renders. Without this, images
/// inside scroll containers bleed out of their bounds (e.g. onto a fixed
/// toolbar or into neighbouring regions).
#[deprecated(since = "0.41.0", note = "種類ごとにまとめて描くと、重なった下の要素の文字や画像が上の箱に透ける (#126)。木の順に描く `bridge::OrderedFrame` を使う")]
pub fn extract_image_batches(list: &RenderList) -> Vec<ImageBatch> {
    let mut batches: Vec<ImageBatch> = Vec::new();
    let mut clip_stack: Vec<sabitori_core::Rect> = Vec::new();

    let mut mask_stack = Vec::new();
    for cmd in &list.commands {
        if skip_mask(cmd, &mut mask_stack) {
            continue;
        }
        match cmd {
            RenderCommand::PushClip(clip_rect) => {
                push_intersected_clip(&mut clip_stack, *clip_rect);
            }
            RenderCommand::PopClip => {
                clip_stack.pop();
            }
            // 種類ごとにまとめて描く古い経路は、グループを回さずその場に描く。
            RenderCommand::PushGroup(_) | RenderCommand::PushMask | RenderCommand::PopGroup => {}
            // パスの塗りは木の順に描く経路だけ。
            RenderCommand::PathFill(_) => {}
            RenderCommand::Image(d) => {
                let Some(inst) = image_instance(d, clip_stack.last()) else { continue };
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
#[deprecated(since = "0.41.0", note = "種類ごとにまとめて描くと、重なった下の要素の文字や画像が上の箱に透ける (#126)。木の順に描く `bridge::OrderedFrame` を使う")]
#[derive(Default)]
pub struct UiDrawLists {
    pub images: Vec<ImageBatch>,
    pub rings: Vec<RingInstance>,
    pub lines: Vec<LineInstance>,
    pub glyphs: Vec<GlyphInstance>,
    /// Goo is not drawn by [`draw_ui_layer`]: it paints *between* the rects,
    /// so it goes to the renderer with them — hand it over with
    /// [`GpuRenderer::set_goo`](sabitori_gpu::GpuRenderer::set_goo) before the
    /// render call.
    pub goo: Vec<GooSlot>,
}

#[allow(deprecated)]
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
        let mut goo = Vec::new();
        let (rects, glyphs, rings, lines) = render_list_to_gpu_with_rings_impl(list, tr, &mut goo);
        (
            rects,
            Self { images: extract_image_batches(list), rings, lines, glyphs, goo },
        )
    }

    /// [`UiDrawLists::extract`] の selection 拡張版。 テキスト選択 / find-in-page を
    /// 扱う呼び出し側は、 次フレームのヒットテスト用に `TextHitLayout` も要る。
    pub fn extract_with_hits(
        list: &RenderList,
        tr: &mut TextRenderer,
    ) -> (Vec<RectInstance>, Self, Vec<TextHitLayout>) {
        let mut goo = Vec::new();
        let (rects, glyphs, rings, lines, layouts) = render_list_to_gpu_with_hits_impl(list, tr, &mut goo);
        (
            rects,
            Self { images: extract_image_batches(list), rings, lines, glyphs, goo },
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
    /// 中身ごと回す要素 ([`Element::rotate`](sabitori_core::Element::rotate)) を描く。
    /// `None` なら回さずその場に描く。
    pub groups: Option<&'a mut sabitori_gpu::GroupRenderer>,
    /// パスの塗り ([`path`](sabitori_core::element::path)) を描く。`None` なら塗らない。
    pub fills: Option<&'a mut sabitori_gpu::FillRenderer>,
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
#[allow(deprecated)]
#[deprecated(since = "0.41.0", note = "種類ごとにまとめて描くと、重なった下の要素の文字や画像が上の箱に透ける (#126)。木の順に描く `bridge::OrderedFrame` を使う")]
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

// ---------------------------------------------------------------------------
// 木の順に描く
// ---------------------------------------------------------------------------

/// 1 枚の中で続けて描くまとまり。種類ごとの instance の並びの範囲を指す。
///
/// 描画命令を**木の順**に並べ、同じ種類が続く所だけをまとめる。種類ごとに全部まとめて
/// 描くと、下の要素の文字や画像が上の要素の地の上に出る (#126)。
#[derive(Clone, Debug, PartialEq)]
pub enum DrawRun {
    Rects(std::ops::Range<u32>),
    Goo(std::ops::Range<u32>),
    /// 同じテクスチャ (`key`) の画像。
    Images { key: String, range: std::ops::Range<u32> },
    Rings(std::ops::Range<u32>),
    Lines(std::ops::Range<u32>),
    Glyphs(std::ops::Range<u32>),
    /// パスの塗りの頂点 (3 つで三角形 1 つ)。
    Fills(std::ops::Range<u32>),
    /// 先に別の絵に描いてから回して貼るまとまり ([`Element::rotate`](sabitori_core::Element::rotate))。
    Group(Box<GroupRun>),
}

/// [`DrawRun::Group`] の中身。
#[derive(Clone, Debug, PartialEq)]
pub struct GroupRun {
    /// 描き先と貼り方 ([`OrderedFrame::groups`]) の番号。内側のグループほど小さい。
    pub slot: u32,
    pub runs: Vec<DrawRun>,
    /// マスク ([`Element::mask`](sabitori_core::Element::mask)): 描き先の番号と中身。
    /// その絵の不透明度をグループの絵に掛ける。
    pub mask: Option<(u32, Vec<DrawRun>)>,
}

impl DrawRun {
    fn span_mut(&mut self) -> Option<&mut std::ops::Range<u32>> {
        match self {
            DrawRun::Rects(r)
            | DrawRun::Goo(r)
            | DrawRun::Images { range: r, .. }
            | DrawRun::Rings(r)
            | DrawRun::Lines(r)
            | DrawRun::Glyphs(r)
            | DrawRun::Fills(r) => Some(r),
            DrawRun::Group(_) => None,
        }
    }
}

/// `run` を並びの末尾に足す。直前と同じ種類 (画像は同じテクスチャ) で続いていれば
/// 1 つにまとめる。空の範囲は足さない。
fn push_run(runs: &mut Vec<DrawRun>, mut run: DrawRun) {
    // グループはまとめない (1 つずつ別の絵)。
    let Some(span) = run.span_mut() else {
        runs.push(run);
        return;
    };
    if span.start >= span.end {
        return;
    }
    let span = span.clone();
    if let Some(last) = runs.last_mut() {
        let same_kind = match (&*last, &run) {
            (DrawRun::Images { key: a, .. }, DrawRun::Images { key: b, .. }) => a == b,
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        };
        if let Some(last_span) = last.span_mut().filter(|s| same_kind && s.end == span.start) {
            last_span.end = span.end;
            return;
        }
    }
    runs.push(run);
}

/// 文字の下に敷く矩形 (選択・検索のハイライト・リンクの下線) を、その文字のグリフの
/// 直前に差し込む。`under` は (グリフの通し番号, 敷く矩形の範囲)。
///
/// 以前はこれらを矩形の並びの末尾に足していた。種類ごとにまとめて描くので「全部の
/// 矩形の後、全部の文字の前」に来ていたが、木の順で描くと、その文字より後に描く
/// 箱の上に出てしまう。
fn splice_under_glyphs(runs: Vec<DrawRun>, mut under: Vec<(u32, std::ops::Range<u32>)>) -> Vec<DrawRun> {
    if under.is_empty() {
        return runs;
    }
    under.sort_by_key(|(at, _)| *at);
    splice_runs(runs, &mut under.into_iter().peekable())
}

/// グリフは木の順に振ってあるので、グループの中も同じ順で潜れば番号順に当たる。
fn splice_runs(
    runs: Vec<DrawRun>,
    pending: &mut std::iter::Peekable<std::vec::IntoIter<(u32, std::ops::Range<u32>)>>,
) -> Vec<DrawRun> {
    let mut out = Vec::with_capacity(runs.len());
    for run in runs {
        let range = match run {
            DrawRun::Glyphs(range) => range,
            DrawRun::Group(mut g) => {
                g.runs = splice_runs(std::mem::take(&mut g.runs), pending);
                if let Some((_, runs)) = g.mask.as_mut() {
                    *runs = splice_runs(std::mem::take(runs), pending);
                }
                out.push(DrawRun::Group(g));
                continue;
            }
            other => {
                out.push(other);
                continue;
            }
        };
        let mut cur = range.start;
        while let Some((at, _)) = pending.peek() {
            if *at < range.start || *at >= range.end {
                break;
            }
            let (at, rects) = pending.next().expect("peeked");
            push_run(&mut out, DrawRun::Glyphs(cur..at));
            push_run(&mut out, DrawRun::Rects(rects));
            cur = at;
        }
        push_run(&mut out, DrawRun::Glyphs(cur..range.end));
    }
    out
}


/// Find-in-page ハイライト rect を text layout から集める。 各 `TextHitLayout`
/// が持つ `HighlightSpec` の byte 範囲を、 selection と同じ per-line union で背景
/// rect に変換する。 `current` の範囲だけ `current_color` で塗る。
/// [`OrderedFrame::extract`] がその文字のグリフの直前に敷く。
pub(crate) fn highlight_rects(
    text_layouts: &[TextHitLayout],
) -> Vec<RectInstance> {
    let mut out: Vec<RectInstance> = Vec::new();
    for layout in text_layouts {
        // Specs paint in order, so a later one covers an earlier one where
        // their ranges overlap — the app decides precedence by call order.
        for hl in &layout.highlight {
            for (ri, &(b0, b1)) in hl.ranges.iter().enumerate() {
                if b1 <= b0 {
                    continue;
                }
                let color = if hl.current == Some(ri) { hl.current_color } else { hl.color };
                // 同じ line_index の hit をまとめて 1 rect にする (selection と同じ)。
                use std::collections::BTreeMap;
                let mut by_line: BTreeMap<u32, (f32, f32, f32, f32)> = BTreeMap::new();
                for hit in &layout.hits {
                    if hit.byte_end <= b0 || hit.byte_start >= b1 {
                        continue;
                    }
                    let entry = by_line
                        .entry(hit.line_index)
                        .or_insert((hit.x, hit.y, hit.x + hit.w, hit.y + hit.h));
                    entry.0 = entry.0.min(hit.x);
                    entry.1 = entry.1.min(hit.y);
                    entry.2 = entry.2.max(hit.x + hit.w);
                    entry.3 = entry.3.max(hit.y + hit.h);
                }
                for (_, (x0, y0, x1, y1)) in by_line {
                    out.push(RectInstance {
                        rect: [x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)],
                        corner_radii: [2.0; 4],
                        fill_color: color.to_array(),
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0,
                        rotation: 0.0,
                        _pad0: 0.0, paint: [0.0; 4], grain: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0; 4],
                        clip_rect: layout
                            .clip_rect
                            .map(|c| [c.origin.x, c.origin.y, c.size.width, c.size.height])
                            .unwrap_or([0.0; 4]),
                    });
                }
            }
        }
    }
    out
}

/// in-body リンク範囲の下線 rect (highlight_rects の下線版)。各範囲を
/// line ごとにまとめ、行の下端に細い下線を link color で敷く。glyph の下に描く。
pub(crate) fn link_rects(
    text_layouts: &[TextHitLayout],
) -> Vec<RectInstance> {
    let mut out: Vec<RectInstance> = Vec::new();
    for layout in text_layouts {
        let Some(ranges) = layout.link_ranges.as_ref() else { continue };
        for r in ranges {
            if r.end <= r.start {
                continue;
            }
            use std::collections::BTreeMap;
            // line_index -> (x0, x1, bottom_y)
            let mut by_line: BTreeMap<u32, (f32, f32, f32)> = BTreeMap::new();
            for hit in &layout.hits {
                if hit.byte_end <= r.start || hit.byte_start >= r.end {
                    continue;
                }
                let entry = by_line
                    .entry(hit.line_index)
                    .or_insert((hit.x, hit.x + hit.w, hit.y + hit.h));
                entry.0 = entry.0.min(hit.x);
                entry.1 = entry.1.max(hit.x + hit.w);
                entry.2 = entry.2.max(hit.y + hit.h);
            }
            for (_, (x0, x1, bottom)) in by_line {
                out.push(RectInstance {
                    rect: [x0, bottom - 1.5, (x1 - x0).max(1.0), 1.5],
                    corner_radii: [0.0; 4],
                    fill_color: r.color.to_array(),
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    gradient_angle: 0.0,
                    rotation: 0.0,
                    _pad0: 0.0, paint: [0.0; 4], grain: [0.0; 4],
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                    gradient_end_color: [0.0; 4],
                    clip_rect: layout
                        .clip_rect
                        .map(|c| [c.origin.x, c.origin.y, c.size.width, c.size.height])
                        .unwrap_or([0.0; 4]),
                });
            }
        }
    }
    out
}

/// パスの塗りを三角形に割る (lyon)。輪郭は閉じたものとして扱う。
fn tessellate_fill(d: &sabitori_core::PathFillDraw, clip_rect: [f32; 4]) -> Vec<sabitori_gpu::FillVertex> {
    use lyon_tessellation::math::point;
    use lyon_tessellation::path::Path;
    use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};
    let mut builder = Path::builder();
    for c in &d.contours {
        let Some(first) = c.first() else { continue };
        builder.begin(point(first.x, first.y));
        for p in &c[1..] {
            builder.line_to(point(p.x, p.y));
        }
        builder.end(true);
    }
    let path = builder.build();
    let mut geom: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let rule = if d.even_odd { FillRule::EvenOdd } else { FillRule::NonZero };
    let ok = FillTessellator::new().tessellate_path(
        &path,
        &FillOptions::tolerance(0.1).with_fill_rule(rule),
        &mut BuffersBuilder::new(&mut geom, |v: FillVertex| v.position().to_array()),
    );
    if ok.is_err() {
        return Vec::new();
    }
    let color = d.color.to_array();
    geom.indices
        .iter()
        .map(|&i| sabitori_gpu::FillVertex { position: geom.vertices[i as usize], color, clip_rect })
        .collect()
}

/// [`GroupInstance::effect`](sabitori_gpu::GroupInstance::effect) の `[1]` に入れる重ね方の番号。
fn blend_index(mode: sabitori_core::BlendMode) -> f32 {
    match mode {
        sabitori_core::BlendMode::Normal => 0.0,
        sabitori_core::BlendMode::Add => 1.0,
        sabitori_core::BlendMode::Screen => 2.0,
        sabitori_core::BlendMode::Multiply => 3.0,
    }
}

/// 1 字ずつ動かす ([`TextDraw::glyph_motion`])。グリフのバイト範囲 (`bytes`、グリフと
/// 同じ順) から字の番号を引き、その字の動きを掛ける: 字の箱の真ん中を中心に拡げて
/// 回し、ずらし、色の不透明度に掛ける。
pub(crate) fn move_glyphs(
    glyphs: &mut [GlyphInstance],
    bytes: &[(u32, u32)],
    content: &str,
    motions: &[sabitori_core::GlyphMotion],
) {
    let starts: Vec<usize> = content.char_indices().map(|(b, _)| b).collect();
    for (g, &(start, _)) in glyphs.iter_mut().zip(bytes) {
        // 合字などで字の途中から始まるグリフは、その字のものとする。
        let ci = match starts.binary_search(&(start as usize)) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let Some(m) = motions.get(ci) else { continue };
        let center = [g.position[0] + g.size[0] * 0.5, g.position[1] + g.size[1] * 0.5];
        g.size = [g.size[0] * m.scale, g.size[1] * m.scale];
        // グリフの四角は左上 (position) まわりに回るので、真ん中が動かない左上を求める。
        let (sin, cos) = m.rotation.sin_cos();
        let (hx, hy) = (g.size[0] * 0.5, g.size[1] * 0.5);
        g.position = [
            center[0] + m.dx - (hx * cos - hy * sin),
            center[1] + m.dy - (hx * sin + hy * cos),
        ];
        g.rotation += m.rotation;
        g.color[3] *= m.opacity;
    }
}

/// 古い経路 (種類ごとにまとめて描く) はグループを使わない。グループの印は読み飛ばし、
/// マスクの中身 (`PushMask` から `PopGroup` まで) は描かない。描かないなら `true`。
fn skip_mask(cmd: &RenderCommand, stack: &mut Vec<bool>) -> bool {
    match cmd {
        RenderCommand::PushGroup(_) => {
            stack.push(false);
            true
        }
        RenderCommand::PushMask => {
            if let Some(m) = stack.last_mut() {
                *m = true;
            }
            true
        }
        RenderCommand::PopGroup => {
            stack.pop();
            true
        }
        _ => stack.iter().any(|&m| m),
    }
}

/// 開いているグループ。
struct OpenGroup {
    /// 外の並びと切り抜き (閉じたら戻す)。
    outer: Vec<DrawRun>,
    outer_clips: Vec<sabitori_core::Rect>,
    inst: sabitori_gpu::GroupInstance,
    /// マスクに入ったら、それまでの中身をここへ退ける。
    content: Option<Vec<DrawRun>>,
}

/// 1 枚ぶんの描画を、木の順のまとまり ([`DrawRun`]) と種類ごとの instance に起こした物。
///
/// 使い方: [`OrderedFrame::extract`] で起こし、文字の下に敷く物があれば
/// [`OrderedFrame::under_text`] で足し、[`OrderedFrame::upload`] で上げてから、
/// 開いたパスの中で [`OrderedFrame::draw`] を呼ぶ (窓なら
/// [`GpuRenderer::render_frame`](sabitori_gpu::GpuRenderer::render_frame) の中)。
/// 矩形と goo は [`RectTarget`](sabitori_gpu::RectTarget) (窓の描画器か、窓を持たない
/// 描画器) が描く。
#[derive(Default)]
pub struct OrderedFrame {
    pub rects: Vec<RectInstance>,
    pub goo: Vec<GooInstance>,
    pub images: Vec<ImageInstance>,
    /// 描く画像のテクスチャ (key ごとに 1 つ)。上げる前に `ensure_texture` する。
    pub image_data: Vec<(String, ImageData)>,
    pub rings: Vec<RingInstance>,
    pub lines: Vec<LineInstance>,
    pub glyphs: Vec<GlyphInstance>,
    /// パスの塗りの頂点。
    pub fills: Vec<sabitori_gpu::FillVertex>,
    pub runs: Vec<DrawRun>,
    /// グループ ([`DrawRun::Group`]) の貼り方。`slot` の順。
    pub groups: Vec<sabitori_gpu::GroupInstance>,
    /// 選択・検索・リンクに使う文字の配置 (読む順)。
    pub text_layouts: Vec<TextHitLayout>,
    /// `text_layouts` の各文字が `glyphs` のどこからどこまでか (`text_idx` → 範囲)。
    pub text_glyphs: HashMap<usize, std::ops::Range<u32>>,
    /// 文字の下に敷く矩形 (グリフの通し番号, 矩形の範囲)。`draw` の前に差し込む。
    under: Vec<(u32, std::ops::Range<u32>)>,
}

impl OrderedFrame {
    /// 描画命令の並びを、描く順に起こす。`lists` は (並び, 選択などの対象にするか) を
    /// 下から順に。対象にする並びの文字には `text_idx` を通し番号で振る (読む順)。
    pub fn extract(lists: &[(&RenderList, bool)], tr: &mut TextRenderer) -> Self {
        // 先に atlas を立て直す (前の枚で溢れていたら、見える字だけで描き直す)。
        tr.maybe_recover_atlas();
        let mut f = OrderedFrame::default();
        let mut text_idx: usize = 0;
        for (list, with_hits) in lists {
            f.extract_one(list, *with_hits, &mut text_idx, tr);
        }
        f
    }

    fn extract_one(&mut self, list: &RenderList, with_hits: bool, text_idx: &mut usize, tr: &mut TextRenderer) {
        let mut clip_stack: Vec<sabitori_core::Rect> = Vec::new();
        // 開いているグループ: (外の並び, 外の切り抜き, 貼り方)。
        let mut open: Vec<OpenGroup> = Vec::new();
        for cmd in &list.commands {
            let clip = clip_stack.last().copied();
            match cmd {
                RenderCommand::PushClip(clip_rect) => push_intersected_clip(&mut clip_stack, *clip_rect),
                RenderCommand::PopClip => {
                    clip_stack.pop();
                }
                // 中身は窓と同じ座標で別の絵に描く。外の切り抜きは貼る絵に掛け、中では
                // 掛けない (回した後の位置で切るため)。
                RenderCommand::PushGroup(g) => {
                    let inst = sabitori_gpu::GroupInstance {
                        pivot_rot: [g.pivot.x, g.pivot.y, g.rotation, 1.0],
                        clip_rect: clip.map_or([0.0; 4], |c| clip_to_array(&c)),
                        effect: [g.blur, blend_index(g.blend), g.zoom, 0.0],
                    };
                    open.push(OpenGroup {
                        outer: std::mem::take(&mut self.runs),
                        outer_clips: std::mem::take(&mut clip_stack),
                        inst,
                        content: None,
                    });
                }
                // マスクも中身と同じ座標で、別の絵に描く。切り抜きは新しく始める。
                RenderCommand::PushMask => {
                    if let Some(g) = open.last_mut() {
                        g.content = Some(std::mem::take(&mut self.runs));
                        clip_stack.clear();
                    }
                }
                RenderCommand::PopGroup => self.close_group(&mut open, &mut clip_stack),
                RenderCommand::Rect(d) => {
                    if clip.is_some_and(|c| is_clipped(&c, &d.rect)) {
                        continue;
                    }
                    let mut inst = rect_to_instance(d);
                    if let Some(c) = clip {
                        inst.clip_rect = clip_to_array(&c);
                    }
                    let at = self.rects.len() as u32;
                    self.rects.push(inst);
                    push_run(&mut self.runs, DrawRun::Rects(at..at + 1));
                }
                RenderCommand::Goo(d) => {
                    if clip.is_some_and(|c| is_clipped(&c, &d.bounds())) {
                        continue;
                    }
                    let mut inst = goo_to_instance(d);
                    if let Some(c) = clip {
                        inst.clip_rect = clip_to_array(&c);
                    }
                    let at = self.goo.len() as u32;
                    self.goo.push(inst);
                    push_run(&mut self.runs, DrawRun::Goo(at..at + 1));
                }
                RenderCommand::Image(d) => {
                    let Some(inst) = image_instance(d, clip.as_ref()) else { continue };
                    if !self.image_data.iter().any(|(k, _)| *k == d.key) {
                        self.image_data.push((d.key.clone(), d.data.clone()));
                    }
                    let at = self.images.len() as u32;
                    self.images.push(inst);
                    push_run(&mut self.runs, DrawRun::Images { key: d.key.clone(), range: at..at + 1 });
                }
                RenderCommand::Ring(d) => {
                    let r = d.outer_radius;
                    let bbox = sabitori_core::Rect::new(d.center.x - r, d.center.y - r, 2.0 * r, 2.0 * r);
                    if clip.is_some_and(|c| is_clipped(&c, &bbox)) {
                        continue;
                    }
                    let mut inst = ring_to_instance(d);
                    if let Some(c) = clip {
                        inst.clip_rect = clip_to_array(&c);
                    }
                    let at = self.rings.len() as u32;
                    self.rings.push(inst);
                    push_run(&mut self.runs, DrawRun::Rings(at..at + 1));
                }
                RenderCommand::Polyline(d) => {
                    if clip.is_some_and(|c| polyline_clipped(&c, &d.points)) {
                        continue;
                    }
                    let mut insts = polyline_to_instances(d);
                    if let Some(c) = clip {
                        let arr = clip_to_array(&c);
                        for i in insts.iter_mut() {
                            i.clip_rect = arr;
                        }
                    }
                    let at = self.lines.len() as u32;
                    self.lines.extend(insts);
                    push_run(&mut self.runs, DrawRun::Lines(at..self.lines.len() as u32));
                }
                RenderCommand::PathFill(d) => {
                    let all: Vec<sabitori_core::Point> = d.contours.iter().flatten().copied().collect();
                    if clip.is_some_and(|c| polyline_clipped(&c, &all)) {
                        continue;
                    }
                    let clip_arr = clip.map_or([0.0; 4], |c| clip_to_array(&c));
                    let at = self.fills.len() as u32;
                    self.fills.extend(tessellate_fill(d, clip_arr));
                    push_run(&mut self.runs, DrawRun::Fills(at..self.fills.len() as u32));
                    // 三角形の縁はぎざぎざなので、不透明な塗りには同じ色の細い線を縁に
                    // 重ねて滑らかにする (半透明だと重なった所が濃くなるので重ねない)。
                    if d.color.a >= 0.999 {
                        let at = self.lines.len() as u32;
                        for c in &d.contours {
                            let mut ring = c.clone();
                            if ring.first() != ring.last() {
                                ring.push(ring[0]);
                            }
                            // 太さはほぼ 0 で、線の縁のぼかし (±0.75px) だけで縁の内外を
                            // なだらかにつなぐ。
                            let mut edge = polyline_to_instances(&PolylineDraw { points: ring, width: 0.2, color: d.color });
                            for e in edge.iter_mut() {
                                e.clip_rect = clip_arr;
                            }
                            self.lines.extend(edge);
                        }
                        push_run(&mut self.runs, DrawRun::Lines(at..self.lines.len() as u32));
                    }
                }
                RenderCommand::CellGrid(d) => {
                    if clip.is_some_and(|c| is_clipped(&c, &d.rect())) {
                        continue;
                    }
                    let at = self.glyphs.len() as u32;
                    self.glyphs.extend(cell_grid_to_glyphs(d, tr, clip));
                    push_run(&mut self.runs, DrawRun::Glyphs(at..self.glyphs.len() as u32));
                }
                RenderCommand::Text(d) => {
                    // 切り抜きで飛ばした文字も番号は進める (スクロールで出入りしても、
                    // 選択が別の文字を指さないように)。
                    let cur_idx = *text_idx;
                    if with_hits {
                        *text_idx += 1;
                    }
                    if clip.is_some_and(|c| is_clipped(&c, &text_cull_rect(d))) {
                        continue;
                    }
                    let max_width = (d.max_width > 0.0 && d.max_width < f32::MAX).then_some(d.max_width);
                    // ハイライトとリンクの下線は、選択の対象でない並び (上掛け・画面外・reel)
                    // でも描く。そのために配置 (hits) が要る。
                    let decorated = !d.highlight.is_empty() || d.link_ranges.is_some();
                    // 縦書きは別の並べ方 (選択・ハイライト・リンクは対応していない)。
                    if d.vertical {
                        let (mut produced, bytes) = tr.prepare_vertical(
                            &d.content, d.position.x, d.position.y, d.max_width, d.max_height,
                            d.font_size, d.color, &color_spans(d), d.bold, d.font_family.as_deref(), d.typo,
                        );
                        if let Some(m) = d.glyph_motion.as_deref() {
                            move_glyphs(&mut produced, &bytes, &d.content, m);
                        }
                        sabitori_text::rotate_glyphs(&mut produced, (d.position.x, d.position.y), d.rotation);
                        sabitori_text::fill_backdrop(&mut produced, d.backdrop);
                        if let Some(c) = clip {
                            let arr = clip_to_array(&c);
                            for g in produced.iter_mut() {
                                g.clip_rect = arr;
                            }
                        }
                        let at = self.glyphs.len() as u32;
                        self.glyphs.extend(produced);
                        push_run(&mut self.runs, DrawRun::Glyphs(at..self.glyphs.len() as u32));
                        continue;
                    }
                    let (mut produced, hits) = if (with_hits && needs_hit_layout(d)) || decorated {
                        let (g, h) = tr.prepare_text_with_hits_spans(
                            &d.content, d.position.x, d.position.y,
                            d.font_size, d.color, &color_spans(d), max_width,
                            d.bold, d.monospace, d.font_family.as_deref(), d.max_lines,
                            d.typo,
                        );
                        (g, Some(h))
                    } else {
                        let g = tr.prepare_text_spans(
                            &d.content, d.position.x, d.position.y,
                            d.font_size, d.color, &color_spans(d), max_width,
                            d.bold, d.monospace, d.font_family.as_deref(), d.max_lines,
                            d.typo,
                        );
                        (g, None)
                    };
                    // 1 字ずつの動き (見た目だけ。当たりの箱は動かさない)。
                    if let Some(m) = d.glyph_motion.as_deref() {
                        move_glyphs(&mut produced, tr.last_glyph_bytes(), &d.content, m);
                    }
                    // 回すのはグリフだけ。当たりの箱は回さない (render_list_to_gpu_with_hits と同じ)。
                    sabitori_text::rotate_glyphs(&mut produced, (d.position.x, d.position.y), d.rotation);
                    sabitori_text::fill_backdrop(&mut produced, d.backdrop);
                    if let Some(c) = clip {
                        let arr = clip_to_array(&c);
                        for g in produced.iter_mut() {
                            g.clip_rect = arr;
                        }
                    }
                    let at = self.glyphs.len() as u32;
                    self.glyphs.extend(produced);
                    let end = self.glyphs.len() as u32;
                    push_run(&mut self.runs, DrawRun::Glyphs(at..end));
                    if let Some(hits) = hits {
                        let layout = TextHitLayout {
                            text_idx: cur_idx,
                            content: d.content.clone(),
                            hits,
                            clip_rect: clip,
                            highlight: d.highlight.clone(),
                            link_ranges: d.link_ranges.clone(),
                            no_select: d.no_select,
                            owner: d.owner.clone(),
                        };
                        // 文字の下に敷く: 検索のハイライト → リンクの下線 (塗る順)。
                        // 選択の塗りは選択の状態を持つ主窓が後から足す。
                        let one = std::slice::from_ref(&layout);
                        let mut under = highlight_rects(one);
                        under.extend(link_rects(one));
                        if !under.is_empty() {
                            let start = self.rects.len() as u32;
                            self.rects.extend(under);
                            self.under.push((at, start..self.rects.len() as u32));
                        }
                        if with_hits {
                            self.text_glyphs.insert(cur_idx, at..end);
                            self.text_layouts.push(layout);
                        }
                    }
                }
            }
        }
        while !open.is_empty() {
            self.close_group(&mut open, &mut clip_stack);
        }
    }

    fn close_group(&mut self, open: &mut Vec<OpenGroup>, clip_stack: &mut Vec<sabitori_core::Rect>) {
        let Some(g) = open.pop() else { return };
        let last = std::mem::replace(&mut self.runs, g.outer);
        *clip_stack = g.outer_clips;
        let (runs, mask) = match g.content {
            Some(content) => {
                // マスクの描き先も 1 枠使う (貼り方は使わない)。
                let slot = self.groups.len() as u32;
                self.groups.push(sabitori_gpu::GroupInstance::default());
                (content, Some((slot, last)))
            }
            None => (last, None),
        };
        let slot = self.groups.len() as u32;
        self.groups.push(g.inst);
        push_run(&mut self.runs, DrawRun::Group(Box::new(GroupRun { slot, runs, mask })));
    }

    /// 文字 `text_idx` の下に矩形を敷く (選択・検索のハイライト・リンクの下線)。
    /// その文字のグリフの直前に描かれる。文字が描かれていなければ (切り抜きの外) 何もしない。
    pub fn under_text(&mut self, text_idx: usize, rects: Vec<RectInstance>) {
        if rects.is_empty() {
            return;
        }
        let Some(glyphs) = self.text_glyphs.get(&text_idx) else { return };
        let at = glyphs.start;
        let start = self.rects.len() as u32;
        self.rects.extend(rects);
        self.under.push((at, start..self.rects.len() as u32));
    }

    /// 最後に (全部の上に) 矩形を描く。
    pub fn on_top(&mut self, rects: Vec<RectInstance>) {
        let start = self.rects.len() as u32;
        self.rects.extend(rects);
        push_run(&mut self.runs, DrawRun::Rects(start..self.rects.len() as u32));
    }

    /// 何も描かないか。
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// 描く物を全部上げる。[`OrderedFrame::draw`] の前に、パスの外で呼ぶ。
    pub fn upload(
        &mut self,
        rt: &mut impl sabitori_gpu::RectTarget,
        r: &mut UiRenderers<'_>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        self.runs = splice_under_glyphs(std::mem::take(&mut self.runs), std::mem::take(&mut self.under));
        rt.upload_rect_instances(device, queue, &self.rects);
        rt.upload_goo_instances(device, queue, &self.goo);
        if let Some(img_r) = r.images.as_deref_mut() {
            for (key, data) in &self.image_data {
                img_r.ensure_texture(device, queue, key, &data.rgba, data.width, data.height);
            }
            img_r.upload(device, queue, &self.images);
        }
        if let Some(ring_r) = r.rings.as_deref_mut() {
            ring_r.upload(device, queue, &self.rings);
        }
        if let Some(line_r) = r.lines.as_deref_mut() {
            line_r.upload(device, queue, &self.lines);
        }
        r.text.upload_glyphs(device, queue, &self.glyphs);
        if let Some(fill_r) = r.fills.as_deref_mut() {
            fill_r.upload(device, queue, &self.fills);
        }

        // グループの中身を、それぞれの絵に先に描く (内側から)。本番のパスはこの後の
        // 提出なので、貼るときには描き終わっている。
        if self.groups.is_empty() {
            return;
        }
        let Some(gr) = r.groups.as_deref_mut() else { return };
        let (w, h) = rt.target_size();
        gr.prepare(device, queue, w, h, rt.scale_factor(), &self.groups);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sabitori_group_encoder"),
        });
        self.render_groups(&self.runs, &*rt, r, &mut encoder);
        queue.submit(std::iter::once(encoder.finish()));
    }

    fn render_groups(
        &self,
        runs: &[DrawRun],
        rt: &impl sabitori_gpu::RectTarget,
        r: &UiRenderers<'_>,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let Some(gr) = r.groups.as_deref() else { return };
        for run in runs {
            let DrawRun::Group(g) = run else { continue };
            self.render_groups(&g.runs, rt, r, encoder);
            self.render_into(gr.target(g.slot as usize), &g.runs, rt, r, encoder);
            gr.apply_effects(encoder, g.slot as usize);
            if let Some((mask_slot, mask_runs)) = &g.mask {
                self.render_groups(mask_runs, rt, r, encoder);
                self.render_into(gr.target(*mask_slot as usize), mask_runs, rt, r, encoder);
            }
        }
    }

    /// `runs` を透明に消した `view` へ描く。
    fn render_into(
        &self,
        view: &wgpu::TextureView,
        runs: &[DrawRun],
        rt: &impl sabitori_gpu::RectTarget,
        r: &UiRenderers<'_>,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sabitori_group_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        self.draw_runs(runs, rt, r, &mut pass);
    }

    /// 木の順に描く。[`OrderedFrame::upload`] の後、開いたパスの中で呼ぶ。
    pub fn draw(
        &self,
        rt: &impl sabitori_gpu::RectTarget,
        r: &UiRenderers<'_>,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        self.draw_runs(&self.runs, rt, r, pass);
    }

    fn draw_runs(
        &self,
        runs: &[DrawRun],
        rt: &impl sabitori_gpu::RectTarget,
        r: &UiRenderers<'_>,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let globals = rt.globals();
        for run in runs {
            match run {
                DrawRun::Rects(range) => rt.draw_rect_range(pass, range.clone()),
                DrawRun::Goo(range) => rt.draw_goo_range(pass, range.clone()),
                DrawRun::Images { key, range } => {
                    if let Some(img_r) = r.images.as_deref() {
                        img_r.draw(pass, globals, key, range.clone());
                    }
                }
                DrawRun::Rings(range) => {
                    if let Some(ring_r) = r.rings.as_deref() {
                        ring_r.draw(pass, globals, range.clone());
                    }
                }
                DrawRun::Lines(range) => {
                    if let Some(line_r) = r.lines.as_deref() {
                        line_r.draw(pass, globals, range.clone());
                    }
                }
                DrawRun::Glyphs(range) => r.text.draw_glyphs(pass, globals, range.clone()),
                DrawRun::Fills(range) => {
                    if let Some(fill_r) = r.fills.as_deref() {
                        fill_r.draw(pass, globals, range.clone());
                    }
                }
                // 描く物が無ければ、回さず・マスクせずにその場に描く。
                DrawRun::Group(g) => match r.groups.as_deref() {
                    Some(gr) => gr.draw(pass, globals, g.slot, g.mask.as_ref().map(|(m, _)| *m)),
                    None => self.draw_runs(&g.runs, rt, r, pass),
                },
            }
        }
    }
}

#[cfg(test)]
mod ordered_tests {
    use super::{push_run, splice_under_glyphs, DrawRun};

    /// 字の番号はバイトではなく `chars()` の順。日本語 (1 字 3 バイト) でも 2 字目は 2 字目。
    #[test]
    fn glyph_motion_is_indexed_by_character() {
        use sabitori_core::GlyphMotion;
        use sabitori_text::GlyphInstance;
        let g = GlyphInstance {
            position: [0.0, 0.0],
            size: [10.0, 10.0],
            uv_rect: [0.0; 4],
            color: [0.0, 0.0, 0.0, 1.0],
            clip_rect: [0.0; 4],
            is_color: 0.0,
            rotation: 0.0,
            backdrop: [0.0; 4],
        };
        let mut glyphs = [g, g, g];
        let motions = [GlyphMotion::default(), GlyphMotion { dy: 5.0, ..Default::default() }];
        super::move_glyphs(&mut glyphs, &[(0, 3), (3, 6), (6, 9)], "あいう", &motions);
        assert_eq!(glyphs.map(|g| g.position[1]), [0.0, 5.0, 0.0]);
    }

    /// 古い経路はグループの印を読み飛ばし、マスクの中身 (PushMask から PopGroup まで)
    /// を描かない。入れ子でも外のマスクが閉じるまで飛ばす。
    #[test]
    fn old_paths_skip_mask_contents() {
        use super::skip_mask;
        use sabitori_core::render_list::{GroupDraw, RectDraw, RenderCommand};
        let g = || RenderCommand::PushGroup(GroupDraw {
            pivot: sabitori_core::Point::ZERO,
            rotation: 0.0,
            blur: 0.0,
            blend: Default::default(),
            zoom: 1.0,
        });
        let rect = || RenderCommand::Rect(RectDraw::default());
        let cmds = [
            g(), rect(), RenderCommand::PushMask, rect(), g(), rect(), RenderCommand::PopGroup, RenderCommand::PopGroup,
            rect(),
        ];
        let mut stack = Vec::new();
        let drawn: Vec<bool> = cmds.iter().map(|c| !skip_mask(c, &mut stack)).collect();
        assert_eq!(drawn, [false, true, false, false, false, false, false, false, true]);
    }

    /// 同じ種類が続けば 1 つにまとめ、種類が変われば分ける (木の順を崩さない)。
    #[test]
    fn runs_merge_only_while_the_kind_continues() {
        let mut runs = Vec::new();
        push_run(&mut runs, DrawRun::Rects(0..1));
        push_run(&mut runs, DrawRun::Rects(1..2));
        push_run(&mut runs, DrawRun::Glyphs(0..5));
        push_run(&mut runs, DrawRun::Rects(2..3));
        push_run(&mut runs, DrawRun::Images { key: "a".into(), range: 0..1 });
        push_run(&mut runs, DrawRun::Images { key: "b".into(), range: 1..2 });
        push_run(&mut runs, DrawRun::Glyphs(5..5)); // 空は足さない
        assert_eq!(
            runs,
            vec![
                DrawRun::Rects(0..2),
                DrawRun::Glyphs(0..5),
                DrawRun::Rects(2..3),
                DrawRun::Images { key: "a".into(), range: 0..1 },
                DrawRun::Images { key: "b".into(), range: 1..2 },
            ]
        );
    }

    /// 文字の下に敷く矩形は、その文字のグリフの直前に入る。同じまとまりの中の
    /// 別の文字の前には入らない。
    #[test]
    fn under_text_rects_go_right_before_their_glyphs() {
        let runs = vec![DrawRun::Rects(0..1), DrawRun::Glyphs(0..10), DrawRun::Rects(1..2), DrawRun::Glyphs(10..12)];
        // 2 番目の文字 (グリフ 4..) と、後ろのまとまりの先頭 (10) に敷く。
        let out = splice_under_glyphs(runs, vec![(10, 5..6), (4, 2..4)]);
        assert_eq!(
            out,
            vec![
                DrawRun::Rects(0..1),
                DrawRun::Glyphs(0..4),
                DrawRun::Rects(2..4),
                DrawRun::Glyphs(4..10),
                DrawRun::Rects(1..2),
                DrawRun::Rects(5..6),
                DrawRun::Glyphs(10..12),
            ]
        );
    }
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
#[allow(deprecated)]
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
            element_index: 0,
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
            color_spans: None,
            highlight: Vec::new(),
            link_ranges: None,
            rotation,
            no_select: false,
            owner: None,
            glyph_motion: None,
            vertical: false,
            backdrop: None,
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
            element_index: 0,
            content: std::sync::Arc::from("あ".repeat(400).as_str()),
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
            color_spans: None,
            highlight: Vec::new(),
            link_ranges: None,
            rotation: 0.0,
            no_select: false,
            owner: None,
            glyph_motion: None,
            vertical: false,
            backdrop: None,
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
#[allow(deprecated)]
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
#[allow(deprecated)]
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

#[cfg(test)]
mod measure_key_tests {
    //! **計測キャッシュの鍵が、区別すべきものを区別すること** ([#80])。
    //!
    //! 鍵から文字列を落として 64bit のハッシュにしたので、材料を 1 つ書き
    //! 忘れると **違うものが同じ測定値を共有する** — 太字だけ違う 2 つの
    //! ラベルが同じ幅になり、片方だけ字が枠からはみ出す。落ちるのは
    //! 「幅がおかしい」という遠い症状なので、ここで直接押さえる。
    //!
    //! [#80]: https://github.com/Mutafika/sabitori/issues/80

    use super::measure_key;
    use sabitori_core::element::{TextAlign, Typography};

    fn base() -> (String, f32, bool, bool, Option<String>, u32, Option<u32>, Typography) {
        ("予約".to_string(), 14.0, false, false, None, u32::MAX, None, Typography::default())
    }

    fn key_of(
        (content, size, bold, mono, family, width, lines, typo): (
            String, f32, bool, bool, Option<String>, u32, Option<u32>, Typography,
        ),
    ) -> u64 {
        measure_key(&content, size, bold, mono, family.as_deref(), width, lines, typo)
    }

    /// 同じ材料なら同じ鍵 (そうでないとキャッシュが 1 度も当たらない)。
    #[test]
    fn the_same_inputs_give_the_same_key() {
        assert_eq!(key_of(base()), key_of(base()));
    }

    /// **測定値を変えうるものは、全部鍵に入っていること。**
    #[test]
    fn everything_that_changes_the_measurement_changes_the_key() {
        let baseline = key_of(base());
        let cases: [(&str, fn(&mut (String, f32, bool, bool, Option<String>, u32, Option<u32>, Typography))); 10] = [
            ("本文", |b| b.0 = "返却".into()),
            ("文字の大きさ", |b| b.1 = 15.0),
            ("太字", |b| b.2 = true),
            ("等幅", |b| b.3 = true),
            ("書体の指定", |b| b.4 = Some("Hiragino".into())),
            ("折り返す幅", |b| b.5 = 240),
            ("行数の上限", |b| b.6 = Some(1)),
            ("字送り", |b| b.7.letter_spacing = 1.0),
            ("行の高さ", |b| b.7.line_height = Some(2.0)),
            ("斜体", |b| b.7.italic = true),
        ];
        for (what, change) in cases {
            let mut b = base();
            change(&mut b);
            assert_ne!(key_of(b), baseline, "{what} を変えても同じ鍵になっている");
        }
    }

    /// 行揃えとウェイトも (上の表に入れると型が合わないので別で)。
    #[test]
    fn alignment_and_weight_are_in_the_key() {
        let baseline = key_of(base());

        let mut b = base();
        b.7.align = TextAlign::Center;
        assert_ne!(key_of(b), baseline, "行揃えが鍵に入っていない");

        let mut b = base();
        b.7.weight = Some(700);
        assert_ne!(key_of(b), baseline, "ウェイトが鍵に入っていない");
    }
}

#[cfg(test)]
mod hit_layout_tests {
    //! **誰も読まない hitbox を作らない** ([#80]) の判定。
    //!
    //! 削りすぎると「選択やリンクが特定の文字でだけ効かない」になり、
    //! 削らなすぎると端末のような画面で要素ごとに 1 確保が戻る。どちらも
    //! 遠い症状で出るので、判定そのものをここで押さえる。
    //!
    //! [#80]: https://github.com/Mutafika/sabitori/issues/80

    use super::needs_hit_layout;
    use sabitori_core::render_list::TextDraw;
    use sabitori_core::{Color, HighlightSpec, LinkRange, Point};

    fn draw() -> TextDraw {
        TextDraw {
            element_index: 0,
            content: std::sync::Arc::from("予約番号 R-0042"),
            position: Point::new(0.0, 0.0),
            max_width: 200.0,
            max_height: 20.0,
            font_size: 14.0,
            color: Color::WHITE,
            bold: false,
            monospace: false,
            font_family: None,
            max_lines: None,
            typo: Default::default(),
            color_spans: None,
            highlight: Vec::new(),
            link_ranges: None,
            rotation: 0.0,
            no_select: false,
            owner: None,
            glyph_motion: None,
            vertical: false,
            backdrop: None,
        }
    }

    /// 選べる文字は作る (既定)。
    #[test]
    fn selectable_text_keeps_its_hitboxes() {
        assert!(needs_hit_layout(&draw()));
    }

    /// 選べず、ハイライトもリンクも無い文字は作らない。
    #[test]
    fn plain_unselectable_text_needs_nothing() {
        let d = TextDraw { no_select: true, ..draw() };
        assert!(!needs_hit_layout(&d));
    }

    /// **選べなくても、敷くものがあるなら要る。** ⌘F の当たりを
    /// `no_select` の見出しに敷く、のような形で効く。
    #[test]
    fn highlights_need_hitboxes_even_when_unselectable() {
        let d = TextDraw {
            no_select: true,
            highlight: vec![HighlightSpec {
                ranges: vec![(0, 4)],
                color: Color::WHITE,
                ..Default::default()
            }],
            ..draw()
        };
        assert!(needs_hit_layout(&d));
    }

    /// リンクも同じ (下線と当たり判定に要る)。
    #[test]
    fn links_need_hitboxes_even_when_unselectable() {
        let d = TextDraw {
            no_select: true,
            link_ranges: Some(vec![LinkRange {
                start: 0,
                end: 4,
                id: "r-0042".into(),
                tooltip: None,
                color: Color::WHITE,
            }]),
            ..draw()
        };
        assert!(needs_hit_layout(&d));
    }
}
