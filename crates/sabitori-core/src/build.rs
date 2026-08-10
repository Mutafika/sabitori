//! Converts an [`Element`] tree into layout (via Taffy) and a flat
//! [`RenderList`] of draw commands.
//!
//! # Usage
//!
//! ```ignore
//! let root = div().w_full().h_full().bg(Color::BLACK).children([
//!     text("Hello").font_size(24.0),
//! ]);
//!
//! let result = build_tree(&root, 800.0, 600.0);
//! // result.render_list  — feed to GPU renderer
//! // result.hit_regions  — feed to input system
//! ```

use crate::element::{
    AlignItems, Cursor, Dimension, Element, ElementKind, ElementStyle,
    FlexDirection, FlexWrap, JustifyContent, Overflow, Position, Typography,
};
use crate::Corners;
use crate::render_list::{ImageDraw, PolylineDraw, RectDraw, RenderCommand, RenderList, RingDraw, TextDraw};
use crate::{Color, Point, Rect};

use crate::TextMetrics;

use taffy::{
    AvailableSpace, LengthPercentage, LengthPercentageAuto,
    Size as TaffySize, Style as TaffyStyle, TaffyTree,
};

// ---------------------------------------------------------------------------
// Hit region for event dispatch
// ---------------------------------------------------------------------------

/// A clickable/hoverable region produced during the build step.
#[derive(Debug)]
pub struct HitRegion {
    /// Absolute bounding box.
    pub rect: Rect,
    /// Index into the original element tree (depth-first).
    pub element_index: usize,
    /// Element ID (if set via `.id("name")`).
    pub id: Option<String>,
    /// Whether this region has a click handler.
    pub clickable: bool,
    /// `element.on_click` が実際に set されている (= app が click を消費するつもり)。
    /// `clickable` は id-bearing でも true になってしまうので、 「本物の click
    /// 対象 vs id 付きの単なる selectable な領域」 を区別するためのフラグ。
    /// テキスト選択を始めるかどうかの判定で使う (handler 無しなら selection 優先)。
    pub has_click_handler: bool,
    /// Whether this region has a hover handler or hover_style.
    pub hoverable: bool,
    /// Whether this region can receive focus.
    pub focusable: bool,
    /// Tooltip text (if set via `.tooltip("...")`).
    pub tooltip: Option<String>,
    /// Drag payload data (if set via `.draggable("...")`).
    pub drag_data: Option<String>,
    /// Whether this region is a drop zone (set via `.droppable()`).
    pub drop_zone: bool,
    /// Cursor preference (set via `.cursor(...)`). `None` means
    /// "no opinion" — runtime falls back to platform default.
    pub cursor: Option<Cursor>,
}

// ---------------------------------------------------------------------------
// Build result
// ---------------------------------------------------------------------------

/// Measured scroll container info from layout.
#[derive(Debug, Clone)]
pub struct ScrollMeasure {
    /// Total content width (furthest-right child edge).
    pub content_width: f32,
    /// Total content height (furthest-down child edge).
    pub content_height: f32,
    /// Viewport width (the scroll container's own width).
    pub viewport_width: f32,
    /// Viewport height (the scroll container's own height).
    pub viewport_height: f32,
}

/// The result of [`build_tree`].
pub struct BuildResult {
    /// Flat list of draw commands (rects + text) in painter order (back to front).
    pub render_list: RenderList,
    /// Overlay draw commands — rendered after all base content (rects + text).
    pub overlay_list: RenderList,
    /// Hit-testable regions (front to back order for picking).
    pub hit_regions: Vec<HitRegion>,
    /// Measured scroll containers: id → (content_height, viewport_height).
    pub scroll_measures: std::collections::HashMap<String, ScrollMeasure>,
    /// Absolute Y of the elements named in the caller's probe set — reported
    /// **even when the element is scrolled out of view**.
    ///
    /// `hit_regions` only carries what is visible (an element fully outside its
    /// parent clip is dropped), so "where is element X" is unanswerable for
    /// off-screen content — which is exactly what scroll-to-element needs.
    /// Layout knows the position regardless; this map just surfaces it.
    /// Empty unless [`build_tree_probed`] / [`build_tree_measured_probed`] was used.
    pub probe_positions: std::collections::HashMap<String, f32>,
}

impl BuildResult {
    /// Topmost hit region under `(x, y)`, if any. Regions are stored
    /// front-to-back, so the first match is the visually topmost one.
    pub fn hit_region_at(&self, x: f32, y: f32) -> Option<&HitRegion> {
        let pt = Point::new(x, y);
        self.hit_regions.iter().find(|r| r.rect.contains(pt))
    }

    /// Whether the UI wants pointer input at `(x, y)` — i.e. the point is
    /// over any interactive region (id-bearing, clickable, hoverable,
    /// focusable, draggable or drop zone). egui の `wants_pointer_input()`
    /// 相当の素材: 3D ビューポートを持つホストアプリは、 これが true の間
    /// カメラ操作を抑止する。 Note: 装飾だけの背景 div は hit region を
    /// 持たない — ポインタをブロックしたいパネルには `.id()` を付けること。
    pub fn wants_pointer(&self, x: f32, y: f32) -> bool {
        self.hit_region_at(x, y).is_some()
    }

    /// Screen-space rect of the (first) hit region carrying `id`, if it
    /// was laid out this frame. ドラッグ系ウィジェット（Slider のトラック
    /// 座標）やオーバーレイのアンカー（Dropdown のトリガー矩形）を、
    /// 埋め込みホストがレイアウト結果から引くための口。
    pub fn region_rect(&self, id: &str) -> Option<Rect> {
        self.hit_regions
            .iter()
            .find(|r| r.id.as_deref() == Some(id))
            .map(|r| r.rect)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Measure text to determine its intrinsic size.
///
/// Implement this trait on your text renderer to get accurate text layout.
/// When not provided, `build_tree` falls back to a character-count estimate.
///
/// Uses `&self` (not `&mut self`) so it can be shared across recursive calls.
/// Implementors should use interior mutability (e.g. `RefCell`) if needed.
pub trait TextMeasure {
    /// Returns the box the text occupies plus its first baseline, in logical
    /// pixels.
    ///
    /// `max_width` constrains the shaping pass so that the height reflects the
    /// true number of wrapped lines. Pass `None` for natural (single-line) width.
    ///
    /// `max_lines`, when `Some(n)`, caps the reported height at `n` lines —
    /// this MUST match the render-time `max_lines` truncation, otherwise a
    /// clamped label is laid out at its full wrapped height and mis-centers in
    /// a fixed-height parent (long text floats out of its box).
    ///
    /// Layout itself only needs [`TextMetrics::size`]; `baseline` is carried
    /// for hosts that anchor text on the baseline rather than the box (CAD/DXF
    /// annotations, PDF output) and would otherwise have to re-shape the string
    /// against their own font stack to find it.
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
    ) -> TextMetrics;
}

/// Build an element tree into render commands and hit regions.
///
/// `viewport_width` and `viewport_height` define the available layout space
/// in logical pixels. Uses a rough estimate for text size.
pub fn build_tree(root: &Element, viewport_width: f32, viewport_height: f32) -> BuildResult {
    build_tree_impl(root, viewport_width, viewport_height, None, &Default::default())
}

/// Like [`build_tree`], but also reports the absolute Y of every element whose
/// id is in `probes`, in [`BuildResult::probe_positions`] — including elements
/// that are scrolled out of view and therefore absent from `hit_regions`.
pub fn build_tree_probed(
    root: &Element,
    viewport_width: f32,
    viewport_height: f32,
    probes: &std::collections::HashSet<String>,
) -> BuildResult {
    build_tree_impl(root, viewport_width, viewport_height, None, probes)
}

/// Build an element tree with accurate text measurement.
///
/// Like [`build_tree`], but uses the provided [`TextMeasure`] to compute
/// exact text dimensions instead of a character-count estimate.
pub fn build_tree_measured(
    root: &Element,
    viewport_width: f32,
    viewport_height: f32,
    measurer: &dyn TextMeasure,
) -> BuildResult {
    build_tree_impl(root, viewport_width, viewport_height, Some(measurer), &Default::default())
}

/// [`build_tree_measured`] + probes. See [`BuildResult::probe_positions`].
pub fn build_tree_measured_probed(
    root: &Element,
    viewport_width: f32,
    viewport_height: f32,
    measurer: &dyn TextMeasure,
    probes: &std::collections::HashSet<String>,
) -> BuildResult {
    build_tree_impl(root, viewport_width, viewport_height, Some(measurer), probes)
}

fn build_tree_impl(
    root: &Element,
    viewport_width: f32,
    viewport_height: f32,
    measurer: Option<&dyn TextMeasure>,
    probes: &std::collections::HashSet<String>,
) -> BuildResult {
    let mut taffy: TaffyTree<TextNodeContext> = TaffyTree::new();

    // Phase 1: create Taffy nodes (bottom-up). Attach text context to leaf
    // text/button nodes so the measure_fn can re-measure under width constraints.
    let root_node = create_taffy_node(&mut taffy, root, &measurer, false);

    // Phase 2: compute layout. If a measurer is available, use it via
    // compute_layout_with_measure; text nodes will be re-shaped under the
    // actual available width so wrapped height is correct.
    let viewport = TaffySize {
        width: AvailableSpace::Definite(viewport_width),
        height: AvailableSpace::Definite(viewport_height),
    };
    if let Some(m) = measurer {
        taffy
            .compute_layout_with_measure(
                root_node,
                viewport,
                |known, avail, _id, ctx, _style| measure_text_leaf(m, known, avail, ctx),
            )
            .expect("Taffy layout computation failed");
    } else {
        taffy
            .compute_layout(root_node, viewport)
            .expect("Taffy layout computation failed");
    }

    // Phase 3: walk tree, collect absolute positions, emit render commands
    let mut render_list = RenderList::new();
    let mut overlay_list = RenderList::new();
    let mut hit_regions: Vec<HitRegion> = Vec::new();
    // Overlay subtrees (.overlay() flag) get their hit regions collected
    // separately so they can be spliced in front of `hit_regions` after
    // the tree walk. This makes a context menu / popup returned from
    // `view()` intercept clicks before underlying UI — without the caller
    // needing to use a dedicated `overlay_view()` hook.
    let mut overlay_hit_regions: Vec<HitRegion> = Vec::new();
    let mut scroll_measures = std::collections::HashMap::new();
    let mut element_counter: usize = 0;
    let mut probe_positions = std::collections::HashMap::new();

    emit_commands(
        &taffy,
        root,
        root_node,
        0.0,
        0.0,
        1.0,
        &mut render_list,
        &mut overlay_list,
        &mut hit_regions,
        &mut overlay_hit_regions,
        &mut scroll_measures,
        &mut element_counter,
        false,
        false,
        1.0,
        None,
        probes,
        &mut probe_positions,
    );

    // Reverse each list so front-most (last drawn) comes first for picking,
    // then prepend overlay regions — they always pick before base regions.
    hit_regions.reverse();
    overlay_hit_regions.reverse();
    let mut combined = Vec::with_capacity(overlay_hit_regions.len() + hit_regions.len());
    combined.extend(overlay_hit_regions);
    combined.extend(hit_regions);
    let hit_regions = combined;

    BuildResult {
        render_list,
        overlay_list,
        hit_regions,
        scroll_measures,
        probe_positions,
    }
}

// ---------------------------------------------------------------------------
// Phase 1: create Taffy nodes
// ---------------------------------------------------------------------------

/// Per-node context Taffy hands back to the measure_fn for leaf text/button
/// elements. For non-text leaves (images, empty divs) we attach `None`.
#[derive(Clone, Debug)]
pub struct TextNodeContext {
    pub content: String,
    pub font_size: f32,
    pub bold: bool,
    pub monospace: bool,
    /// Specific font family override (see `ElementStyle::font_family`).
    pub font_family: Option<String>,
    /// Padding added around measured text content. Only set for button-kind
    /// leaves so the final size matches `content + padding`.
    pub padding: (f32, f32, f32, f32), // (top, right, bottom, left)
    /// Extended typography (weight / letter-spacing / line-height) so the
    /// measure pass matches what the render pass will shape.
    pub typo: Typography,
    /// Line cap (`max_lines`) so the measured height matches the render-time
    /// truncation. Without it a clamped label measures at full wrapped height.
    pub max_lines: Option<u32>,
}

/// Taffy measure_fn callback for leaf text nodes. Calls the user-supplied
/// `TextMeasure` with the `available_space` width, so shaping uses the
/// constrained width and returns a height that reflects actual wrapped lines.
fn measure_text_leaf(
    measurer: &dyn TextMeasure,
    known: TaffySize<Option<f32>>,
    avail: TaffySize<AvailableSpace>,
    ctx: Option<&mut TextNodeContext>,
) -> TaffySize<f32> {
    if let (Some(w), Some(h)) = (known.width, known.height) {
        return TaffySize { width: w, height: h };
    }
    let Some(ctx) = ctx else {
        // Non-text leaf: defer to known / zero.
        return TaffySize {
            width: known.width.unwrap_or(0.0),
            height: known.height.unwrap_or(0.0),
        };
    };

    // Translate Taffy's AvailableSpace to a max_width for shaping.
    // MinContent → unconstrained (we'll still clamp later).
    // MaxContent → unconstrained (report natural width).
    // Definite(w) → use w minus padding for the content shape.
    let (pad_top, pad_right, pad_bottom, pad_left) = ctx.padding;
    // Shape at the node's already-resolved width when Taffy has fixed it (explicit
    // `.w()`), and only fall back to `avail`. Deriving max_width solely from `avail`
    // shaped a width-constrained text node (known.width=Some, avail=MaxContent)
    // unconstrained → measured as 1 line while it rendered wrapped → overlapping
    // rows/paragraphs (surfaced on iOS, where Taffy takes that probe path).
    let max_width = match known.width {
        Some(w) => Some((w - pad_left - pad_right).max(0.0)),
        None => match avail.width {
            AvailableSpace::Definite(w) => Some((w - pad_left - pad_right).max(0.0)),
            AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
        },
    };

    let metrics = measurer.measure(
        &ctx.content,
        ctx.font_size,
        ctx.bold,
        ctx.monospace,
        ctx.font_family.as_deref(),
        max_width,
        ctx.max_lines,
        ctx.typo,
    );

    // Layout uses the box only — the baseline rides along for hosts that need
    // it (see `TextMeasure::measure`) and never affects taffy.
    let size = metrics.size;
    let w = known.width.unwrap_or(size.width + pad_left + pad_right);
    let h = known.height.unwrap_or(size.height + pad_top + pad_bottom);
    TaffySize { width: w, height: h }
}

fn create_taffy_node(
    taffy: &mut TaffyTree<TextNodeContext>,
    element: &Element,
    measurer: &Option<&dyn TextMeasure>,
    parent_scrolls: bool,
) -> taffy::NodeId {
    // A scroll container's direct children must keep their natural size along the
    // scroll axis. Otherwise the default `flex_shrink: 1` squeezes them down to
    // the (definite) viewport height, so the measured content extent never
    // exceeds the viewport and there's nothing to scroll — `overflow_scroll`
    // silently does nothing. Pinning shrink to 0 lets content overflow + scroll.
    let this_scrolls = element.style.overflow == Overflow::Scroll;

    // Recursively create child nodes first
    let child_ids: Vec<taffy::NodeId> = element
        .children
        .iter()
        .map(|child| create_taffy_node(taffy, child, measurer, this_scrolls))
        .collect();

    let mut style = convert_to_taffy_style(&element.style, &element.kind, *measurer);
    if parent_scrolls {
        style.flex_shrink = 0.0;
    }

    // Attach measure context for leaf text/button nodes so compute_layout_with_measure
    // can re-shape under actual available width.
    let context: Option<TextNodeContext> = if child_ids.is_empty() {
        match &element.kind {
            ElementKind::Text { content } => Some(TextNodeContext {
                content: content.clone(),
                font_size: element.style.font_size,
                bold: element.style.bold,
                monospace: element.style.monospace,
                font_family: element.style.font_family.clone(),
                padding: (0.0, 0.0, 0.0, 0.0),
                typo: element.style.typography(),
                max_lines: element.style.max_lines,
            }),
            ElementKind::Button { label, .. } => {
                let pad = resolve_edges_px(&element.style.padding);
                Some(TextNodeContext {
                    content: label.clone(),
                    font_size: element.style.font_size,
                    bold: element.style.bold,
                    monospace: element.style.monospace,
                    font_family: element.style.font_family.clone(),
                    padding: (pad.1, pad.2, pad.3, pad.0), // (top, right, bottom, left)
                    typo: element.style.typography(),
                    max_lines: element.style.max_lines,
                })
            }
            _ => None,
        }
    } else {
        None
    };

    if child_ids.is_empty() {
        match context {
            Some(ctx) => taffy
                .new_leaf_with_context(style, ctx)
                .expect("Failed to create Taffy text leaf"),
            None => taffy.new_leaf(style).expect("Failed to create Taffy leaf"),
        }
    } else {
        taffy
            .new_with_children(style, &child_ids)
            .expect("Failed to create Taffy node")
    }
}

/// Count elements in a subtree without emitting any render commands.
/// Used to keep element_counter consistent when culling off-screen children.
fn count_elements(element: &Element, counter: &mut usize) {
    *counter += 1;
    for child in &element.children {
        count_elements(child, counter);
    }
}

// ---------------------------------------------------------------------------
// Phase 3: emit render commands
// ---------------------------------------------------------------------------

// Accumulated clip rect (`parent_clip`) from ancestors with overflow
// Hidden/Scroll is threaded through recursion and intersected into each
// hit region. Without this, a scrolled child's hit rect can extend past
// the scroll container and absorb clicks on fixed siblings (toolbars,
// headers) sitting above the container.
/// Record probe positions inside a subtree that is being culled from drawing.
///
/// Culling skips `emit_commands` entirely, so a probed element that happens to be
/// a direct child of a scroll container (or inside a zero-area clip) would never
/// be recorded — even though layout knows exactly where it is. This walks such a
/// subtree for positions only: no draw commands, no hit regions, no measuring.
/// Callers must skip it when `probes` is empty (the common case pays nothing).
fn record_probes(
    taffy: &TaffyTree<TextNodeContext>,
    element: &Element,
    taffy_node: taffy::NodeId,
    parent_x: f32,
    parent_y: f32,
    probes: &std::collections::HashSet<String>,
    probe_positions: &mut std::collections::HashMap<String, f32>,
) {
    let Ok(layout) = taffy.layout(taffy_node) else { return };
    let style = &element.style;
    let abs_x = parent_x + layout.location.x + style.translate_x;
    let abs_y = parent_y + layout.location.y + style.translate_y;
    if let Some(id) = element.id.as_deref() {
        if probes.contains(id) {
            probe_positions.insert(id.to_string(), abs_y);
        }
    }
    let children = taffy.children(taffy_node).unwrap_or_default();
    for (i, child) in element.children.iter().enumerate() {
        if let Some(&node) = children.get(i) {
            record_probes(taffy, child, node, abs_x, abs_y, probes, probe_positions);
        }
    }
}

fn emit_commands(
    taffy: &TaffyTree<TextNodeContext>,
    element: &Element,
    taffy_node: taffy::NodeId,
    parent_x: f32,
    parent_y: f32,
    parent_opacity: f32,
    render_list: &mut RenderList,
    overlay_list: &mut RenderList,
    hit_regions: &mut Vec<HitRegion>,
    overlay_hit_regions: &mut Vec<HitRegion>,
    scroll_measures: &mut std::collections::HashMap<String, ScrollMeasure>,
    element_counter: &mut usize,
    in_overlay: bool,
    // `user-select: none` の継承状態。 ある要素で `.no_select()` が立つと、 そこから
    // 下の TextDraw が全部 `no_select` になる (CSS の `user-select` と同じ継承)。
    parent_no_select: bool,
    // 祖先が課した視覚 scale の累積 (根は 1.0)。 レイアウト空間の px を画面 px に
    // 直す係数で、 opacity と同じく乗算で下りていく。
    parent_scale: f32,
    parent_clip: Option<Rect>,
    probes: &std::collections::HashSet<String>,
    probe_positions: &mut std::collections::HashMap<String, f32>,
) {
    let layout = taffy.layout(taffy_node).expect("Missing layout");
    let style = &element.style;
    // `scale` cascades multiplicatively like opacity: `parent_scale` is what
    // ancestors already imposed, `scale` adds this element's own on top and is
    // what everything *inside* it (children, text, radii, borders) is drawn at.
    // Layout is never redone — taffy measured everything at 1.0, so scaling is
    // a pure post-layout transform and siblings never move (the whole point of
    // a press affordance: the button shrinks, the row does not reflow).
    let scale = parent_scale * style.scale;
    // `translate_x/y` are visual-only offsets applied AFTER taffy layout —
    // they shift the rendered rect + hit region without telling taffy, so
    // sibling layout stays put while a hover spring "pulls" or "lifts"
    // this element. Children inherit the shifted origin via abs_x/abs_y.
    // Layout-space offsets arrive in unscaled px, so the *inherited* factor
    // converts them to screen px; this element's own factor is not in play yet.
    let slot_x = parent_x + (layout.location.x + style.translate_x) * parent_scale;
    let slot_y = parent_y + (layout.location.y + style.translate_y) * parent_scale;
    let w = layout.size.width * scale;
    let h = layout.size.height * scale;
    // Own scale pivots on the element's center — growing on hover must not
    // shift the top-left corner, or a 1.1 hover would visibly slide the widget.
    let abs_x = slot_x + (layout.size.width * parent_scale - w) * 0.5;
    let abs_y = slot_y + (layout.size.height * parent_scale - h) * 0.5;
    // Opacity cascades multiplicatively: a parent at 0.4 with a child at
    // 0.8 produces an effective 0.32 — matching CSS / SwiftUI / every
    // other tree-based UI system. Without this, fading a popup panel
    // out would only fade its OWN bg, leaving inner text/icons fully
    // opaque against the wallpaper as the panel slid away (the
    // "naked content during close" bug).
    let effective_opacity = parent_opacity * style.opacity;

    let index = *element_counter;
    *element_counter += 1;

    // Determine which list to write to: if this element or any ancestor is
    // overlay, all commands go to the overlay list.
    let use_overlay = in_overlay || element.overlay;
    let no_select = parent_no_select || element.no_select;
    let target = if use_overlay { &mut *overlay_list } else { &mut *render_list };

    // Skip invisible elements
    if w <= 0.0 || h <= 0.0 {
        // A zero-sized CLIPPING container (overflow Hidden/Scroll) shows
        // nothing: cull the entire subtree. Recursing here would emit the
        // children UNCLIPPED — taffy still lays them out at their natural
        // content size inside the zero-sized box, and since this early-out
        // never pushes a PushClip, hundreds of scroll rows would render
        // over the rest of the screen (the "flex_1 scroll list squeezed to
        // zero height leaks its rows everywhere" bug).
        if matches!(style.overflow, Overflow::Hidden | Overflow::Scroll) {
            for child in &element.children {
                count_elements(child, element_counter);
            }
            if !probes.is_empty() {
                record_probes(taffy, element, taffy_node, parent_x, parent_y, probes, probe_positions);
            }
            return;
        }
        // overflow: visible — children may legitimately stick out of a
        // zero-sized wrapper; still need to recurse for counter consistency.
        let taffy_children = taffy.children(taffy_node).unwrap_or_default();
        for (i, child_elem) in element.children.iter().enumerate() {
            if let Some(&child_taffy) = taffy_children.get(i) {
                emit_commands(
                    taffy, child_elem, child_taffy,
                    abs_x, abs_y, effective_opacity, render_list, overlay_list,
                    hit_regions, overlay_hit_regions, scroll_measures, element_counter, use_overlay,
                    no_select, scale,
                    parent_clip,
                    probes, probe_positions,
                );
            }
        }
        return;
    }

    let rect = Rect::new(abs_x, abs_y, w, h);

    // Determine effective background color
    let bg = match &element.kind {
        ElementKind::Button { accent, .. } => accent.unwrap_or(style.background),
        _ => style.background,
    };

    // Emit rect draw if the element has any visual content
    let has_visual = bg.a > 0.0
        || style.border_width > 0.0
        || style.shadow.is_some();

    if has_visual {
        let (shadow_color, shadow_offset, shadow_blur, shadow_spread) =
            match &style.shadow {
                Some(s) => (s.color, s.offset, s.blur, s.spread),
                None => (Color::TRANSPARENT, Point::ZERO, 0.0, 0.0),
            };

        target.commands.push(RenderCommand::Rect(RectDraw {
            rect,
            corner_radii: scale_corners(style.corner_radius, scale),
            fill_color: apply_opacity(bg, effective_opacity),
            border_color: apply_opacity(style.border_color, effective_opacity),
            border_width: style.border_width * scale,
            shadow_color: apply_opacity(shadow_color, effective_opacity),
            shadow_offset: Point::new(shadow_offset.x * scale, shadow_offset.y * scale),
            shadow_blur: shadow_blur * scale,
            shadow_spread: shadow_spread * scale,
            opacity: effective_opacity,
            gradient_angle: style.gradient_angle,
            gradient_end_color: apply_opacity(style.gradient_end, effective_opacity),
            rotation: style.rotation,
        }));
    }

    // Emit text draw for Text and Button elements
    match &element.kind {
        ElementKind::Text { content } => {
            // padding はレイアウト空間の px なので、描画位置に使う前に scale する
            // （箱だけ縮んで中の字が元の位置に残る、を防ぐ）。
            let padding = scale_edges(resolve_edges_px(&style.padding), scale);
            target.commands.push(RenderCommand::Text(TextDraw {
                content: content.clone(),
                position: Point::new(abs_x + padding.0, abs_y + padding.1),
                max_width: (w - padding.0 - padding.2).max(0.0),
                max_height: (h - padding.1 - padding.3).max(0.0),
                font_size: style.font_size * scale,
                color: apply_opacity(style.color, effective_opacity),
                bold: style.bold,
                monospace: style.monospace,
                font_family: style.font_family.clone(),
                max_lines: style.max_lines,
                typo: style.typography(),
                highlight: style.highlight.clone(),
                link_ranges: style.link_ranges.clone(),
                // 同じ `style.rotation` が上の RectDraw にも渡っているが、
                // ピボットが違う (rect = 中心 / text = 原点)。TextDraw::rotation 参照。
                rotation: style.rotation,
                no_select,
            }));
        }
        ElementKind::Button { label, .. } => {
            // Button label is centered text
            let padding = scale_edges(resolve_edges_px(&style.padding), scale);
            target.commands.push(RenderCommand::Text(TextDraw {
                content: label.clone(),
                position: Point::new(abs_x + padding.0, abs_y + padding.1),
                max_width: (w - padding.0 - padding.2).max(0.0),
                max_height: (h - padding.1 - padding.3).max(0.0),
                font_size: style.font_size * scale,
                color: apply_opacity(style.color, effective_opacity),
                bold: style.bold,
                monospace: style.monospace,
                font_family: style.font_family.clone(),
                max_lines: style.max_lines,
                typo: style.typography(),
                highlight: style.highlight.clone(),
                link_ranges: style.link_ranges.clone(),
                rotation: style.rotation,
                // A button label is a control's caption, not prose — dragging
                // across a toolbar should never leave it highlighted. Always
                // non-selectable, regardless of the inherited flag.
                no_select: true,
            }));
        }
        ElementKind::Div => {}
        ElementKind::Arc(arc) => {
            // Center the arc inside the layout rect's bounding square.
            let cx = abs_x + w * 0.5;
            let cy = abs_y + h * 0.5;
            let outer_radius = (w.min(h) * 0.5).max(0.0);
            let inner_radius = (outer_radius - arc.thickness * scale).max(0.0);
            target.commands.push(RenderCommand::Ring(RingDraw {
                center: Point::new(cx, cy),
                outer_radius,
                inner_radius,
                start_angle: arc.start_angle,
                sweep_angle: arc.sweep_angle,
                value: arc.value.clamp(0.0, 1.0),
                fill_color: apply_opacity(arc.fill_color, effective_opacity),
                track_color: apply_opacity(arc.track_color, effective_opacity),
            }));
        }
        ElementKind::Polyline(pl) => {
            // Points are local to the element box; offset to absolute px.
            if pl.points.len() >= 2 {
                let pts = pl
                    .points
                    .iter()
                    .map(|(px, py)| Point::new(abs_x + *px * scale, abs_y + *py * scale))
                    .collect();
                target.commands.push(RenderCommand::Polyline(PolylineDraw {
                    points: pts,
                    width: (pl.width * scale).max(0.0),
                    color: apply_opacity(pl.color, effective_opacity),
                }));
            }
        }
        ElementKind::Image { key, data } => {
            target.commands.push(RenderCommand::Image(ImageDraw {
                key: key.clone(),
                data: data.clone(),
                rect,
                corner_radii: scale_corners(style.corner_radius, scale),
                opacity: effective_opacity,
                object_fit: style.object_fit,
            }));
        }
    }

    // Register hit region if interactive. The rect is clipped against the
    // accumulated ancestor clip so a scrolled child cannot absorb clicks on
    // a sibling that sits outside the scroll container (fixed toolbars,
    // headers, neighbouring panels). Without this, hit regions use their
    // pre-clip rect and the scroll-adjusted position can extend past the
    // container bounds.
    // Probe recording happens BEFORE the clip test below: the whole point is to
    // answer "where is X" for elements the clip would otherwise erase. `rect` is
    // the pre-clip, scroll-adjusted absolute box, so the caller can convert it to
    // content space with the container's own rect + scroll offset.
    if !probes.is_empty() {
        if let Some(id) = element.id.as_deref() {
            if probes.contains(id) {
                probe_positions.insert(id.to_string(), rect.origin.y);
            }
        }
    }

    let clickable = element.on_click.is_some() || element.id.is_some() || element.drag_data.is_some();
    let hoverable = element.on_hover.is_some() || element.hover_style.is_some() || element.id.is_some() || element.tooltip.is_some() || element.drop_zone;
    if clickable || hoverable || element.focusable {
        let hit_rect = match parent_clip {
            Some(clip) => match rect.intersect(&clip) {
                Some(r) => r,
                None => {
                    // Fully clipped — no part of this element is interactive.
                    // Still need to fall through so children recurse (they
                    // may be positioned differently, e.g., via their own
                    // absolute layout, though currently children inherit).
                    Rect::new(0.0, 0.0, 0.0, 0.0)
                }
            },
            None => rect,
        };
        if hit_rect.size.width > 0.0 && hit_rect.size.height > 0.0 {
            let region = HitRegion {
                rect: hit_rect,
                element_index: index,
                id: element.id.clone(),
                clickable,
                has_click_handler: element.on_click.is_some(),
                hoverable,
                focusable: element.focusable,
                tooltip: element.tooltip.clone(),
                drag_data: element.drag_data.clone(),
                drop_zone: element.drop_zone,
                cursor: element.cursor,
            };
            if use_overlay {
                overlay_hit_regions.push(region);
            } else {
                hit_regions.push(region);
            }
        }
    }

    // Clip children if overflow is not Visible
    let clips = matches!(style.overflow, Overflow::Hidden | Overflow::Scroll);
    let target_list = if use_overlay { &mut *overlay_list } else { &mut *render_list };
    let own_clip: Option<Rect> = if clips {
        // Use content box (container minus padding) for clip rect
        let padding = resolve_edges_px(&style.padding);
        let clip_rect = Rect::new(
            rect.origin.x + padding.0,
            rect.origin.y + padding.1,
            (rect.size.width - padding.0 - padding.2).max(0.0),
            (rect.size.height - padding.1 - padding.3).max(0.0),
        );
        // Degenerate content box (padding ate the whole container): nothing
        // inside can be visible. Cull the subtree instead of pushing a
        // zero-sized clip — downstream, a zero-sized clip rect would collide
        // with the GPU instances' `w==0||h==0 == "unclipped"` sentinel and
        // DISABLE clipping for everything inside, leaking the children over
        // the whole screen.
        if clip_rect.size.width <= 0.0 || clip_rect.size.height <= 0.0 {
            // Still record the scroll measure (content extent from the taffy
            // layouts) so managed scroll state doesn't go stale.
            if style.overflow == Overflow::Scroll {
                if let Some(ref id) = element.id {
                    let taffy_children = taffy.children(taffy_node).unwrap_or_default();
                    let mut content_w: f32 = 0.0;
                    let mut content_h: f32 = 0.0;
                    for &child_taffy in &taffy_children {
                        if let Ok(cl) = taffy.layout(child_taffy) {
                            content_w = content_w.max(cl.location.x + cl.size.width);
                            content_h = content_h.max(cl.location.y + cl.size.height);
                        }
                    }
                    scroll_measures.insert(id.clone(), ScrollMeasure {
                        content_width: content_w,
                        content_height: content_h,
                        viewport_width: clip_rect.size.width,
                        viewport_height: clip_rect.size.height,
                    });
                }
            }
            for child in &element.children {
                count_elements(child, element_counter);
            }
            return;
        }
        target_list.commands.push(RenderCommand::PushClip(clip_rect));
        Some(clip_rect)
    } else {
        None
    };
    // Clip passed to children = parent_clip ∩ own_clip. If either is None the
    // other wins; if both present, take the intersection (empty ⇒ children
    // get a zero rect and drop all their hit regions).
    let child_clip = match (parent_clip, own_clip) {
        (None, c) | (c, None) => c,
        (Some(p), Some(o)) => p.intersect(&o).or(Some(Rect::new(0.0, 0.0, 0.0, 0.0))),
    };

    // Scroll offset — shift children when overflow is Hidden or Scroll
    let child_offset_x = if matches!(style.overflow, Overflow::Hidden | Overflow::Scroll) { -style.scroll_x } else { 0.0 };
    let child_offset_y = if matches!(style.overflow, Overflow::Hidden | Overflow::Scroll) { -style.scroll_y } else { 0.0 };

    // Recurse into children
    let taffy_children = taffy.children(taffy_node).unwrap_or_default();

    // For scroll containers: skip children that are entirely outside the viewport
    let is_scroll = style.overflow == Overflow::Scroll;
    let viewport_top = if is_scroll { style.scroll_y } else { 0.0 };
    let viewport_bottom = if is_scroll { style.scroll_y + h } else { f32::MAX };

    // Measure scroll container content extent (both axes).
    let mut max_child_bottom: f32 = 0.0;
    let mut max_child_right: f32 = 0.0;

    for (i, child_elem) in element.children.iter().enumerate() {
        if let Some(&child_taffy) = taffy_children.get(i) {
            let child_layout = taffy.layout(child_taffy).expect("Missing child layout");

            // Track content extent for scroll containers
            if is_scroll {
                let child_bottom = child_layout.location.y + child_layout.size.height;
                let child_right = child_layout.location.x + child_layout.size.width;
                if child_bottom > max_child_bottom {
                    max_child_bottom = child_bottom;
                }
                if child_right > max_child_right {
                    max_child_right = child_right;
                }

                // Cull children outside scroll viewport (vertical only;
                // horizontal culling would need symmetric info and is a rare
                // win compared to the risk of culling a wide row in a
                // horizontal scroller).
                let child_top = child_layout.location.y;
                if child_bottom < viewport_top || child_top > viewport_bottom {
                    count_elements(child_elem, element_counter);
                    // Culled from drawing, but still locatable — that is the whole
                    // point of a probe (scroll-to-element targets off-screen rows).
                    if !probes.is_empty() {
                        record_probes(
                            taffy, child_elem, child_taffy,
                            abs_x + child_offset_x * scale, abs_y + child_offset_y * scale,
                            probes, probe_positions,
                        );
                    }
                    continue;
                }
            }

            emit_commands(
                taffy, child_elem, child_taffy,
                abs_x + child_offset_x * scale, abs_y + child_offset_y * scale,
                effective_opacity,
                render_list, overlay_list,
                hit_regions, overlay_hit_regions, scroll_measures, element_counter, use_overlay,
                no_select, scale,
                child_clip,
                probes, probe_positions,
            );
        }
    }

    // Record measured content extent for scroll containers
    if is_scroll {
        if let Some(ref id) = element.id {
            scroll_measures.insert(id.clone(), ScrollMeasure {
                content_width: max_child_right,
                content_height: max_child_bottom,
                viewport_width: w,
                viewport_height: h,
            });
        }

        // Framework-drawn scrollbar thumb (`.scrollbar(color)`): a thin
        // rounded bar at the container's right edge, shown only while the
        // content overflows vertically. Drawn after the children (so it
        // paints over them) at *container* coords — unlike the children it
        // is NOT offset by `scroll_y`, so it stays put while content moves.
        // `style.scroll_y` here is the managed state's animated offset
        // (patched in before build), so the thumb glides with the smooth-
        // scroll spring. Indicator only: no hit region is registered, so
        // click/wheel routing is untouched.
        if let Some(thumb) = element.style.scrollbar_thumb {
            if max_child_bottom > h + 1.0 && h > 0.0 {
                let thumb_h = (h / max_child_bottom * h).max(20.0).min(h);
                let max_scroll = max_child_bottom - h;
                let norm = (style.scroll_y / max_scroll).clamp(0.0, 1.0);
                let ty = rect.origin.y + norm * (h - thumb_h);
                let target = if use_overlay { &mut *overlay_list } else { &mut *render_list };
                target.commands.push(RenderCommand::Rect(RectDraw {
                    rect: Rect::new(rect.origin.x + w - 6.0, ty, 4.0, thumb_h),
                    corner_radii: Corners::all(2.0),
                    fill_color: apply_opacity(thumb, effective_opacity),
                    border_color: Color::TRANSPARENT,
                    border_width: 0.0,
                    shadow_color: Color::TRANSPARENT,
                    shadow_offset: Point::ZERO,
                    shadow_blur: 0.0,
                    shadow_spread: 0.0,
                    opacity: effective_opacity,
                    gradient_angle: 0.0,
                    gradient_end_color: Color::TRANSPARENT,
                    rotation: 0.0,
                }));
            }
            // Horizontal scrollbar — mirror of the vertical one, along the
            // bottom edge, shown while content overflows horizontally (carousels
            // / timelines). Same indicator-only semantics (no hit region).
            if max_child_right > w + 1.0 && w > 0.0 {
                let thumb_w = (w / max_child_right * w).max(20.0).min(w);
                let max_scroll = max_child_right - w;
                let norm = (style.scroll_x / max_scroll).clamp(0.0, 1.0);
                let tx = rect.origin.x + norm * (w - thumb_w);
                let target = if use_overlay { &mut *overlay_list } else { &mut *render_list };
                target.commands.push(RenderCommand::Rect(RectDraw {
                    rect: Rect::new(tx, rect.origin.y + h - 6.0, thumb_w, 4.0),
                    corner_radii: Corners::all(2.0),
                    fill_color: apply_opacity(thumb, effective_opacity),
                    border_color: Color::TRANSPARENT,
                    border_width: 0.0,
                    shadow_color: Color::TRANSPARENT,
                    shadow_offset: Point::ZERO,
                    shadow_blur: 0.0,
                    shadow_spread: 0.0,
                    opacity: effective_opacity,
                    gradient_angle: 0.0,
                    gradient_end_color: Color::TRANSPARENT,
                    rotation: 0.0,
                }));
            }
        }
    }

    if clips {
        let target_list = if use_overlay { &mut *overlay_list } else { &mut *render_list };
        target_list.commands.push(RenderCommand::PopClip);
    }
}

// ---------------------------------------------------------------------------
// Style conversion: Element -> Taffy
// ---------------------------------------------------------------------------

fn measure_or_estimate(
    content: &str,
    style: &ElementStyle,
    measurer: Option<&dyn TextMeasure>,
) -> (f32, f32) {
    if let Some(m) = measurer {
        // No width constraint here — this is only used as a min-size hint.
        // Actual wrapped sizing happens via `compute_layout_with_measure`.
        let metrics = m.measure(content, style.font_size, style.bold, style.monospace, style.font_family.as_deref(), None, style.max_lines, style.typography());
        (metrics.size.width, metrics.size.height)
    } else {
        let base = if style.monospace { 0.64 } else { 0.55 };
        let factor = if style.bold { base * 1.05 } else { base };
        let w = content.len() as f32 * style.font_size * factor;
        let h = style.font_size * 1.3;
        (w, h)
    }
}

fn convert_to_taffy_style(
    style: &ElementStyle,
    kind: &ElementKind,
    measurer: Option<&dyn TextMeasure>,
) -> TaffyStyle {
    // For text elements, provide a minimum intrinsic size based on font metrics.
    //
    // When a measurer is available the actual size comes from Taffy's
    // `compute_layout_with_measure` → `measure_text_leaf` callback, which shapes
    // text under the available width. In that case we DO NOT pre-set min_w/min_h
    // from a natural-width measurement, because that would force the parent to
    // allocate a single-line width and defeat wrapping.
    //
    // When no measurer is provided (tests, no renderer) we still fall back to
    // the character-count estimate so simple layouts remain reasonable.
    let (min_w, min_h) = match kind {
        ElementKind::Text { content } => {
            if measurer.is_some() {
                (
                    convert_dimension(style.min_width),
                    convert_dimension(style.min_height),
                )
            } else {
                let (tw, th) = measure_or_estimate(content, style, measurer);
                (
                    if style.width == Dimension::Auto && style.min_width == Dimension::Auto {
                        taffy::Dimension::Length(tw)
                    } else {
                        convert_dimension(style.min_width)
                    },
                    if style.height == Dimension::Auto && style.min_height == Dimension::Auto {
                        taffy::Dimension::Length(th)
                    } else {
                        convert_dimension(style.min_height)
                    },
                )
            }
        }
        ElementKind::Button { label, .. } => {
            if measurer.is_some() {
                (
                    convert_dimension(style.min_width),
                    convert_dimension(style.min_height),
                )
            } else {
                let (tw, th) = measure_or_estimate(label, style, measurer);
                let pad = resolve_edges_px(&style.padding);
                (
                    if style.width == Dimension::Auto && style.min_width == Dimension::Auto {
                        taffy::Dimension::Length(tw + pad.0 + pad.2)
                    } else {
                        convert_dimension(style.min_width)
                    },
                    if style.height == Dimension::Auto && style.min_height == Dimension::Auto {
                        taffy::Dimension::Length(th + pad.1 + pad.3)
                    } else {
                        convert_dimension(style.min_height)
                    },
                )
            }
        }
        // Containers get a min size of 0 rather than CSS's `auto`.
        //
        // Under `min-*: auto` a flex item refuses to shrink below its content,
        // so a `grow(1.0)` row inflates to its content's height and any child
        // sizing off it — `h_full()`, or an `overflow_scroll` pane taking the
        // row's height — resolves against the inflated number. The pane's
        // viewport then equals its content and there is nothing left to clip,
        // so scrolling silently does nothing while the layout still *looks*
        // right. The same root bites horizontally: text in a flex row won't
        // wrap, because the row won't shrink below the text's natural width.
        //
        // Setting the min on the pane itself does not help — the ancestor is
        // what inflated — so the workaround is to find the right ancestor and
        // put `min_h(0)` there, which is neither discoverable nor local. See
        // https://github.com/Mutafika/sabitori/issues/60.
        //
        // `min-*: auto` exists so browsers don't collapse text to nothing.
        // Sabitori is an application toolkit, not a browser, and its text
        // elements carry their own intrinsic minimum (the `Text` / `Button`
        // arms above), so containers gain nothing from it and pay the trap.
        // "Fits in, or gets clipped" is the honest behaviour here.
        ElementKind::Div
        | ElementKind::Image { .. }
        | ElementKind::Arc(_)
        | ElementKind::Polyline(_) => (
            convert_min_dimension(style.min_width),
            convert_min_dimension(style.min_height),
        ),
    };

    TaffyStyle {
        display: taffy::Display::Flex,
        position: match style.position {
            Position::Relative => taffy::Position::Relative,
            Position::Absolute => taffy::Position::Absolute,
        },
        flex_direction: match style.flex_direction {
            FlexDirection::Row => taffy::FlexDirection::Row,
            FlexDirection::Column => taffy::FlexDirection::Column,
            FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
            FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
        },
        flex_wrap: match style.flex_wrap {
            FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
            FlexWrap::Wrap => taffy::FlexWrap::Wrap,
            FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
        },
        align_items: Some(match style.align_items {
            AlignItems::Stretch => taffy::AlignItems::Stretch,
            AlignItems::Start => taffy::AlignItems::FlexStart,
            AlignItems::End => taffy::AlignItems::FlexEnd,
            AlignItems::Center => taffy::AlignItems::Center,
        }),
        justify_content: Some(match style.justify_content {
            JustifyContent::Start => taffy::JustifyContent::FlexStart,
            JustifyContent::End => taffy::JustifyContent::FlexEnd,
            JustifyContent::Center => taffy::JustifyContent::Center,
            JustifyContent::SpaceBetween => taffy::JustifyContent::SpaceBetween,
            JustifyContent::SpaceAround => taffy::JustifyContent::SpaceAround,
            JustifyContent::SpaceEvenly => taffy::JustifyContent::SpaceEvenly,
        }),
        flex_grow: style.flex_grow,
        flex_shrink: style.flex_shrink,
        flex_basis: convert_dimension(style.flex_basis),
        gap: TaffySize {
            width: LengthPercentage::Length(style.gap),
            height: LengthPercentage::Length(style.gap),
        },
        size: TaffySize {
            width: convert_dimension(style.width),
            height: convert_dimension(style.height),
        },
        min_size: TaffySize {
            width: min_w,
            height: min_h,
        },
        max_size: TaffySize {
            width: convert_dimension(style.max_width),
            height: convert_dimension(style.max_height),
        },
        padding: taffy::Rect {
            top: convert_lp(style.padding.top),
            right: convert_lp(style.padding.right),
            bottom: convert_lp(style.padding.bottom),
            left: convert_lp(style.padding.left),
        },
        margin: taffy::Rect {
            top: convert_lpa(style.margin.top),
            right: convert_lpa(style.margin.right),
            bottom: convert_lpa(style.margin.bottom),
            left: convert_lpa(style.margin.left),
        },
        inset: taffy::Rect {
            top: convert_lpa(style.inset_top),
            right: convert_lpa(style.inset_right),
            bottom: convert_lpa(style.inset_bottom),
            left: convert_lpa(style.inset_left),
        },
        overflow: taffy::Point {
            x: match style.overflow {
                Overflow::Visible => taffy::Overflow::Visible,
                Overflow::Hidden => taffy::Overflow::Hidden,
                Overflow::Scroll => taffy::Overflow::Scroll,
            },
            y: match style.overflow {
                Overflow::Visible => taffy::Overflow::Visible,
                Overflow::Hidden => taffy::Overflow::Hidden,
                Overflow::Scroll => taffy::Overflow::Scroll,
            },
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn convert_dimension(d: Dimension) -> taffy::Dimension {
    match d {
        Dimension::Auto => taffy::Dimension::Auto,
        Dimension::Px(v) => taffy::Dimension::Length(v),
        Dimension::Percent(v) => taffy::Dimension::Percent(v / 100.0),
    }
}

/// Like [`convert_dimension`], but resolves an unset (`Auto`) minimum to `0`
/// instead of CSS's automatic minimum size. Used for container min-sizes — see
/// the comment at the `Div` arm of `convert_to_taffy_style` for why.
fn convert_min_dimension(d: Dimension) -> taffy::Dimension {
    match d {
        Dimension::Auto => taffy::Dimension::Length(0.0),
        other => convert_dimension(other),
    }
}

fn convert_lp(d: Dimension) -> LengthPercentage {
    match d {
        Dimension::Px(v) => LengthPercentage::Length(v),
        Dimension::Percent(v) => LengthPercentage::Percent(v / 100.0),
        Dimension::Auto => LengthPercentage::Length(0.0),
    }
}

fn convert_lpa(d: Dimension) -> LengthPercentageAuto {
    match d {
        Dimension::Auto => LengthPercentageAuto::Auto,
        Dimension::Px(v) => LengthPercentageAuto::Length(v),
        Dimension::Percent(v) => LengthPercentageAuto::Percent(v / 100.0),
    }
}

/// Resolve EdgeDimensions to (left, top, right, bottom) pixel values.
/// Auto is treated as 0.
fn resolve_edges_px(
    edges: &crate::element::EdgeDimensions,
) -> (f32, f32, f32, f32) {
    (
        dim_to_px(edges.left),
        dim_to_px(edges.top),
        dim_to_px(edges.right),
        dim_to_px(edges.bottom),
    )
}

/// 4 辺の px を一様に scale する。`resolve_edges_px` の戻り値 (left, top, right,
/// bottom) をそのまま受ける。
fn scale_edges(e: (f32, f32, f32, f32), scale: f32) -> (f32, f32, f32, f32) {
    (e.0 * scale, e.1 * scale, e.2 * scale, e.3 * scale)
}

/// 角丸半径を一様に scale する。箱だけ縮んで角丸が据え置きだと、小さい箱ほど
/// 丸が効きすぎて別の形に見える。
fn scale_corners(c: Corners<f32>, scale: f32) -> Corners<f32> {
    Corners {
        top_left: c.top_left * scale,
        top_right: c.top_right * scale,
        bottom_right: c.bottom_right * scale,
        bottom_left: c.bottom_left * scale,
    }
}

fn dim_to_px(d: Dimension) -> f32 {
    match d {
        Dimension::Px(v) => v,
        _ => 0.0,
    }
}

fn apply_opacity(color: Color, opacity: f32) -> Color {
    if opacity >= 1.0 {
        color
    } else {
        // Premultiply RGB by opacity, not just alpha. The GPU
        // pipeline composites with `PREMULTIPLIED_ALPHA_BLENDING`
        // and the rect/ring/image shaders all output `color *
        // sdf_coverage` — i.e. they treat the incoming color as
        // un-premultiplied with respect to its OWN alpha but
        // expect the caller to pre-bake any opacity-style fade.
        // Without this, mid-fade frames composite as
        // `bright_rgb + bg * (1 - small_alpha)` → overbright
        // "white flash" while a popup fades in or out.
        Color::new(
            color.r * opacity,
            color.g * opacity,
            color.b * opacity,
            color.a * opacity,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{button, div, text, Px};

    /// Marker color identifying the scrolled rows in the clip-leak tests.
    const ROW_COLOR: Color = Color::new(0.12, 0.34, 0.56, 1.0);

    /// bamiri の dock_panel 形そのまま:
    /// 固定 root → menu/toolbar → flex_1 の middle row →
    /// `w(Px).h_full()` パネル → タイトルバー + flex_1 padded column(body)
    /// → 兄弟数個 + `flex_1().overflow_scroll()` リスト(大量の行)。
    /// `sibling_h` で兄弟の高さを変え、スクロールリストの flex 割当てを
    /// 健全(>0)/ゼロに振り分ける。
    fn dock_panel_tree(sibling_h: f32, row_count: usize) -> Element {
        let mut rows = Vec::new();
        for i in 0..row_count {
            rows.push(
                div()
                    .id(format!("row-{i}"))
                    .w_full()
                    .h(Px(24.0))
                    .bg(ROW_COLOR),
            );
        }
        let mut body: Vec<Element> = Vec::new();
        for _ in 0..7 {
            body.push(div().w_full().h(Px(sibling_h)).bg(Color::BLACK));
        }
        body.push(
            div()
                .id("scroll-list")
                .w_full()
                .flex_1()
                .flex_col()
                .overflow_scroll()
                .children(rows),
        );
        let panel = div()
            .id("panel")
            .w(Px(260.0))
            .h_full()
            .flex_col()
            .children([
                div().w_full().h(Px(28.0)).bg(Color::BLACK), // title bar
                div()
                    .w_full()
                    .flex_1()
                    .p(Px(8.0))
                    .gap(6.0)
                    .flex_col()
                    .children(body),
            ]);
        let middle = div()
            .w_full()
            .flex_1()
            .flex_row()
            .child(div().flex_1()) // transparent viewport
            .child(panel);
        div()
            .w(Px(800.0))
            .h(Px(600.0))
            .flex_col()
            .children([
                div().w_full().h(Px(28.0)).bg(Color::BLACK), // menu bar
                div().w_full().h(Px(40.0)).bg(Color::BLACK), // toolbar
                middle,
                div().w_full().h(Px(28.0)).bg(Color::BLACK), // snap toolbar
                div().w_full().h(Px(34.0)).bg(Color::BLACK), // command row
            ])
    }

    /// Walk a render list the way the GPU bridge does: maintain the running
    /// clip-stack intersection and return `(active_clip, rect)` for every
    /// Rect command. Also asserts no PushClip command is degenerate — a
    /// zero-sized clip rect collides with the GPU `w==0||h==0 == unclipped`
    /// sentinel and would disable clipping entirely.
    fn rects_with_active_clip(list: &RenderList) -> Vec<(Option<Rect>, RectDraw)> {
        let mut clip_stack: Vec<Rect> = Vec::new();
        let mut out = Vec::new();
        for cmd in &list.commands {
            match cmd {
                RenderCommand::PushClip(r) => {
                    assert!(
                        r.size.width > 0.0 && r.size.height > 0.0,
                        "degenerate PushClip emitted: {r:?} — collides with the \
                         GPU zero-size 'unclipped' sentinel"
                    );
                    let merged = match clip_stack.last() {
                        Some(p) => p.intersect(r).unwrap_or(Rect::new(
                            r.origin.x, r.origin.y, 0.0, 0.0,
                        )),
                        None => *r,
                    };
                    clip_stack.push(merged);
                }
                RenderCommand::PopClip => {
                    clip_stack.pop();
                }
                RenderCommand::Rect(d) => {
                    out.push((clip_stack.last().copied(), d.clone()));
                }
                _ => {}
            }
        }
        out
    }

    /// 健全な形(兄弟が小さく、スクロールリストに正の高さが残る)では、
    /// 行は必ずスクロールコンテナの layout rect 以下の clip を持ち、
    /// コンテナ下端より下の行は cull されること。
    #[test]
    fn nested_flex_scroll_rows_clipped_to_container() {
        let root = dock_panel_tree(20.0, 100);
        let result = build_tree(&root, 800.0, 600.0);

        let m = result
            .scroll_measures
            .get("scroll-list")
            .expect("scroll container must be measured");
        assert!(m.viewport_height > 0.0, "viewport must have height");
        assert!(
            m.content_height > m.viewport_height,
            "rows must overflow the viewport"
        );

        let rows: Vec<_> = rects_with_active_clip(&result.render_list)
            .into_iter()
            .filter(|(_, d)| d.fill_color == ROW_COLOR)
            .collect();
        assert!(!rows.is_empty(), "some rows must render");
        assert!(
            rows.len() < 100,
            "rows below the container bottom must be culled, got {}",
            rows.len()
        );
        for (clip, d) in &rows {
            let c = clip.expect("every scrolled row must carry an active clip");
            assert!(
                c.size.height <= m.viewport_height + 0.5,
                "row clip height {} exceeds scroll viewport {}",
                c.size.height,
                m.viewport_height
            );
            assert!(
                c.origin.y + c.size.height <= 600.0 + 0.5,
                "row clip extends past the window bottom: {c:?}"
            );
            // Rows laid out entirely below the clip bottom must not be
            // emitted at all.
            assert!(
                d.rect.origin.y <= c.origin.y + c.size.height + 0.5,
                "row at y={} is fully below the clip bottom {} but was emitted",
                d.rect.origin.y,
                c.origin.y + c.size.height
            );
        }
    }

    /// バグ再現: 兄弟が大きく flex_1 スクロールリストが高さ 0 に潰れた場合。
    /// 修正前は zero-size 早期 return が PushClip を一切積まずに子へ再帰し、
    /// 数百行が画面全体に clip 無しで漏れていた(bamiri で観測されたリーク)。
    /// 高さ 0 のスクロールコンテナは「何も表示しない」が正しい。
    #[test]
    fn zero_height_scroll_container_culls_rows_instead_of_leaking() {
        let root = dock_panel_tree(70.0, 100);
        let result = build_tree(&root, 800.0, 600.0);

        let rows: Vec<_> = rects_with_active_clip(&result.render_list)
            .into_iter()
            .filter(|(_, d)| d.fill_color == ROW_COLOR)
            .collect();
        for (clip, d) in &rows {
            let c = clip.unwrap_or_else(|| {
                panic!(
                    "row at y={} emitted WITHOUT an active clip — scroll \
                     container leak (rows bleed over the whole screen)",
                    d.rect.origin.y
                )
            });
            assert!(
                c.size.width > 0.0 && c.size.height > 0.0,
                "row carries a degenerate clip {c:?} — the GPU sentinel would \
                 disable clipping for it"
            );
        }
        // No row may extend below the window bottom (unclipped overflow).
        for (clip, d) in &rows {
            let c = clip.unwrap();
            let visible_bottom =
                (d.rect.origin.y + d.rect.size.height).min(c.origin.y + c.size.height);
            assert!(
                visible_bottom <= 600.0 + 0.5,
                "row visibly extends past the window bottom: rect={:?} clip={:?}",
                d.rect,
                c
            );
        }
    }

    /// padding がコンテナを食い潰して content box が高さ 0 になった clip
    /// コンテナも、子を漏らさず cull すること(zero-size sentinel 衝突の別経路)。
    #[test]
    fn degenerate_padded_clip_culls_children() {
        let root = div()
            .w(Px(200.0))
            .h(Px(10.0))
            .p(Px(8.0)) // 8+8 > 10 → content box height 0
            .overflow_hidden()
            .child(div().w(Px(100.0)).h(Px(100.0)).bg(ROW_COLOR));
        let result = build_tree(&root, 800.0, 600.0);
        let rows: Vec<_> = rects_with_active_clip(&result.render_list)
            .into_iter()
            .filter(|(_, d)| d.fill_color == ROW_COLOR)
            .collect();
        assert!(
            rows.is_empty(),
            "children of a zero-area clip container must be culled, got {} leaked",
            rows.len()
        );
    }

    // A fixed-height overflow_scroll box must measure content_height from its
    // children's natural size, NOT cap it to the viewport. This regressed when
    // the scroll box was a default (flex-row) div: align-items:stretch pinned
    // the single child to the viewport height so content could never overflow.
    // A flex_col scroll box measures real content (here 10×50px = 500px > 300px
    // viewport). Explicit child heights → no text measurer needed.
    #[test]
    fn flex_col_scroll_box_measures_content_above_viewport() {
        let mut log = div().flex_col().gap(8.0).w_full().p(Px(10.0));
        for _ in 0..10 {
            log = log.child(div().w_full().h(Px(50.0)).bg(Color::WHITE));
        }
        let root = div()
            .id("transcript")
            .flex_col()
            .w(Px(400.0))
            .h(Px(300.0))
            .overflow_scroll()
            .child(log);

        let result = build_tree(&root, 800.0, 600.0);
        let m = result.scroll_measures.get("transcript").expect("measured");
        let max_scroll = (m.content_height - m.viewport_height).max(0.0);
        assert!(
            max_scroll > 0.0,
            "scrollable content must exceed viewport: content={} viewport={}",
            m.content_height, m.viewport_height
        );
    }

    // BUGS.md「flex_1().overflow_scroll() 単独だと scroll がロックされる」の再現テスト。
    // 明示の Px 高さ無しで、 flex_1 が割り当てた残り高さが viewport_height として
    // 測定され、 子の合計がそれを超えて content_height に記録されること。
    #[test]
    fn flex_grow_scroll_box_measures_viewport_and_content() {
        let mut items = Vec::new();
        for _ in 0..50 {
            items.push(div().w_full().h(Px(40.0)).bg(Color::WHITE));
        }
        let root = div()
            .w(Px(400.0))
            .h(Px(600.0))
            .flex_col()
            .children([
                div().w_full().h(Px(100.0)).bg(Color::BLACK), // header
                div()
                    .id("body-scroll")
                    .flex_1()
                    .flex_col()
                    .overflow_scroll()
                    .children(items),
            ]);

        let result = build_tree(&root, 400.0, 600.0);
        let m = result.scroll_measures.get("body-scroll").expect("measured");
        assert!(
            (m.viewport_height - 500.0).abs() < 1.0,
            "flex_1 viewport should be 600-100=500, got {}",
            m.viewport_height
        );
        assert!(
            m.content_height > 1990.0,
            "content should be 50*40=2000, got {}",
            m.content_height
        );
    }

    // BUGS.md の報告そのままの形: flex_1 ラッパーの中に header + flex_1 スクロール。
    // ラッパー側も basis-auto だと content 高さで膨らみ、 全体が連鎖的に壊れていた。
    #[test]
    fn nested_flex_grow_scroll_box_matches_bugs_md_shape() {
        let mut items = Vec::new();
        for _ in 0..70 {
            items.push(div().w_full().h(Px(40.0)).bg(Color::WHITE));
        }
        let root = div()
            .w(Px(800.0))
            .h(Px(600.0))
            .flex_col()
            .child(
                div().flex_1().flex_col().children([
                    div().w_full().h(Px(48.0)).bg(Color::BLACK), // header
                    div().w_full().h(Px(1.0)).bg(Color::WHITE),  // hsep
                    div()
                        .id("article-scroll")
                        .flex_1()
                        .flex_col()
                        .overflow_scroll()
                        .children(items),
                ]),
            );

        let result = build_tree(&root, 800.0, 600.0);
        let m = result.scroll_measures.get("article-scroll").expect("measured");
        let expected_viewport = 600.0 - 48.0 - 1.0;
        assert!(
            (m.viewport_height - expected_viewport).abs() < 1.0,
            "viewport should be {expected_viewport}, got {}",
            m.viewport_height
        );
        let max_scroll = (m.content_height - m.viewport_height).max(0.0);
        assert!(
            max_scroll > 2000.0,
            "70*40=2800 content in {expected_viewport} viewport must scroll, max_scroll={max_scroll}"
        );
    }

    #[test]
    fn empty_div_produces_no_commands() {
        let root = div();
        let result = build_tree(&root, 800.0, 600.0);
        // An empty transparent div has no visual content
        assert_eq!(result.render_list.rect_count(), 0);
        assert_eq!(result.render_list.text_count(), 0);
    }

    #[test]
    fn colored_div_produces_rect() {
        let root = div()
            .w(Px(100.0))
            .h(Px(50.0))
            .bg(Color::WHITE);

        let result = build_tree(&root, 800.0, 600.0);
        assert_eq!(result.render_list.rect_count(), 1);

        let rect_cmd = result.render_list.rects().next().unwrap();
        assert!((rect_cmd.rect.size.width - 100.0).abs() < 0.1);
        assert!((rect_cmd.rect.size.height - 50.0).abs() < 0.1);
    }

    #[test]
    fn text_produces_text_command() {
        let root = text("Hello world").font_size(20.0).color(Color::WHITE);
        let result = build_tree(&root, 800.0, 600.0);
        assert_eq!(result.render_list.text_count(), 1);

        let text_cmd = result.render_list.texts().next().unwrap();
        assert_eq!(text_cmd.content, "Hello world");
        assert!((text_cmd.font_size - 20.0).abs() < 0.01);
        assert!(!text_cmd.no_select, "既定は選択可能のまま");
    }

    /// `.no_select()` は CSS の `user-select` と同じく subtree に継承する。
    /// 継承しないと「パネル配下の全 text に付けて回る」になり、実際 consumer 側は
    /// 毎フレーム木を書き換える羽目になっていた。
    #[test]
    fn no_select_inherits_to_the_whole_subtree() {
        let root = div().no_select().children([
            text("chrome"),
            div().children([text("nested chrome")]),
        ]);
        let result = build_tree(&root, 800.0, 600.0);
        let texts: Vec<_> = result.render_list.texts().collect();
        assert_eq!(texts.len(), 2);
        assert!(texts.iter().all(|t| t.no_select), "子孫まで全部 no_select");
    }

    /// 兄弟には漏れない — 本文は選択できたまま、chrome だけ切れる、が要件。
    #[test]
    fn no_select_does_not_leak_to_siblings() {
        let root = div().children([
            div().no_select().children([text("sidebar")]),
            div().children([text("prose")]),
        ]);
        let result = build_tree(&root, 800.0, 600.0);
        let texts: Vec<_> = result.render_list.texts().collect();
        assert_eq!(texts.len(), 2);
        assert!(texts[0].no_select, "sidebar");
        assert!(!texts[1].no_select, "prose は選択可能のまま");
    }

    // -----------------------------------------------------------------
    // hover / active の畳み込み (#3)
    // -----------------------------------------------------------------

    fn folded(mut el: Element, hovered: Option<&str>, pressed: Option<&str>) -> Element {
        crate::element::apply_state_styles(
            &mut el,
            &hovered.map(str::to_string),
            &pressed.map(str::to_string),
        );
        el
    }

    /// 本題の回帰: `.active()` が畳まれること。押下状態を追う所が無かった頃は、
    /// この style は誰にも読まれずに捨てられていた。
    #[test]
    fn active_style_is_folded_while_pressed() {
        let el = || div().id("save").bg(Color::BLACK).active(|s| s.scale(0.95));

        assert_eq!(folded(el(), None, None).style.scale, 1.0, "素のときは素通し");
        assert_eq!(folded(el(), None, Some("save")).style.scale, 0.95);
        assert_eq!(folded(el(), None, Some("other")).style.scale, 1.0, "他人の押下では効かない");
    }

    /// 押下は hover に勝つ (`NodeStyle::effective_style` と同じ規約)。
    #[test]
    fn active_wins_over_hover() {
        let el = div()
            .id("b")
            .hover(|s| s.scale(1.1).bg(Color::WHITE))
            .active(|s| s.scale(0.95));
        let out = folded(el, Some("b"), Some("b"));

        assert_eq!(out.style.scale, 0.95, "押下中は active の値");
        assert_eq!(
            out.style.background, Color::WHITE,
            "active が触っていないフィールドは hover の値が残る"
        );
    }

    /// transitions を持つ要素でも、StyleAnimator が扱わないフィールドは即時に
    /// 効く。ここを飛ばすと `.spring_transition()` を足した瞬間に `.active()` の
    /// scale が黙って死ぬ — 一番たちの悪い形になる。
    #[test]
    fn animated_elements_still_get_the_fields_the_animator_cannot_reach() {
        let el = div()
            .id("b")
            .bg(Color::BLACK)
            .spring_transition(300.0, 25.0)
            .active(|s| s.scale(0.95).bg(Color::WHITE));
        let out = folded(el, None, Some("b"));

        assert_eq!(out.style.scale, 0.95, "scale は animator が扱わない → 即時");
        assert_eq!(
            out.style.background, Color::BLACK,
            "bg は animator の担当 → ここでは触らない (補間を潰さない)"
        );
    }

    /// レイアウトを変えるフィールドも畳める。畳みは build の前に走るので、
    /// taffy は畳んだ後の値をそのまま測る。
    #[test]
    fn state_styles_can_change_layout() {
        let el = div().id("b").w(Px(100.0)).h(Px(40.0)).active(|s| s.w(Px(120.0)));
        let out = folded(el, None, Some("b"));
        let result = build_tree(&out, 800.0, 600.0);
        let region = result.hit_regions.iter().find(|r| r.id.as_deref() == Some("b")).unwrap();

        assert_eq!(region.rect.size.width, 120.0);
    }

    /// `button()` は既定で押し込みの手応えを持つ — consumer が毎回組み立てないで済む。
    #[test]
    fn button_has_a_default_press_affordance() {
        let b = button("OK").id("ok");
        assert!(b.active_style.is_some(), "button に既定の active_style が無い");

        let pressed = folded(button("OK").id("ok"), None, Some("ok"));
        assert!(pressed.style.scale < 1.0, "押下で縮むこと");
        let hovered = folded(button("OK").id("ok"), Some("ok"), None);
        assert!(hovered.style.scale > 1.0, "hover で少し持ち上がること");
    }

    // -----------------------------------------------------------------
    // scale — レイアウトを動かさない視覚 transform (#3)
    // -----------------------------------------------------------------

    /// scale は要素の**中心**を軸に効く。左上を軸にすると、hover で 1.1 に
    /// なった瞬間にウィジェットが右下へずれて見える。
    #[test]
    fn scale_pivots_on_the_center() {
        let root = div().w(Px(200.0)).h(Px(100.0)).children([
            div().w(Px(100.0)).h(Px(100.0)).bg(Color::WHITE).scaled(0.5),
        ]);
        let result = build_tree(&root, 800.0, 600.0);
        let r = result.render_list.rects().next().unwrap().rect;

        assert_eq!(r.size.width, 50.0);
        assert_eq!(r.size.height, 50.0);
        // 中心 (50, 50) は動かない → 原点は (25, 25)。
        assert_eq!(r.origin.x, 25.0);
        assert_eq!(r.origin.y, 25.0);
    }

    /// レイアウトはやり直さない。押されたボタンが縮んでも隣の行は動かない、が
    /// 押下フィードバックの前提。
    #[test]
    fn scale_does_not_move_siblings() {
        let scaled = div().w(Px(200.0)).flex_row().children([
            div().w(Px(100.0)).h(Px(40.0)).bg(Color::WHITE).scaled(0.5),
            div().w(Px(100.0)).h(Px(40.0)).bg(Color::BLACK),
        ]);
        let plain = div().w(Px(200.0)).flex_row().children([
            div().w(Px(100.0)).h(Px(40.0)).bg(Color::WHITE),
            div().w(Px(100.0)).h(Px(40.0)).bg(Color::BLACK),
        ]);
        let a = build_tree(&scaled, 800.0, 600.0);
        let b = build_tree(&plain, 800.0, 600.0);
        let sib_a = a.render_list.rects().nth(1).unwrap().rect;
        let sib_b = b.render_list.rects().nth(1).unwrap().rect;

        assert_eq!(sib_a.origin.x, sib_b.origin.x, "隣は動かない");
        assert_eq!(sib_a.size.width, sib_b.size.width, "隣は縮まない");
    }

    /// subtree 全部に乗る — 子の位置・寸法も、文字の大きさも。箱だけ縮んで
    /// 中身が元寸のままだと、押し込みではなく「枠が欠けた」ように見える。
    #[test]
    fn scale_cascades_to_children_and_text() {
        let root = div().w(Px(200.0)).h(Px(200.0)).scaled(0.5).children([
            div().w(Px(100.0)).h(Px(100.0)).bg(Color::WHITE).children([
                text("x").font_size(20.0),
            ]),
        ]);
        let result = build_tree(&root, 800.0, 600.0);

        let child = result.render_list.rects().next().unwrap().rect;
        assert_eq!(child.size.width, 50.0, "子も半分になる");
        let t = result.render_list.texts().next().unwrap();
        assert_eq!(t.font_size, 10.0, "文字も半分になる");
    }

    /// hit region も一緒に変換されること。見えている場所と押せる場所がずれると、
    /// 縮んだボタンの縁が「押せるのに反応しない」帯になる。
    #[test]
    fn scale_transforms_the_hit_region() {
        let root = div().w(Px(200.0)).h(Px(100.0)).children([
            div().id("btn").w(Px(100.0)).h(Px(100.0)).bg(Color::WHITE).scaled(0.5),
        ]);
        let result = build_tree(&root, 800.0, 600.0);
        let region = result
            .hit_regions
            .iter()
            .find(|r| r.id.as_deref() == Some("btn"))
            .expect("hit region for btn");

        assert_eq!(region.rect.origin.x, 25.0);
        assert_eq!(region.rect.size.width, 50.0);
    }

    /// 既定は 1.0 = 素通し。既存アプリの見た目が 1px も動かないこと。
    #[test]
    fn unscaled_geometry_is_untouched() {
        let root = div().w(Px(120.0)).h(Px(60.0)).bg(Color::WHITE).rounded_px(8.0);
        let result = build_tree(&root, 800.0, 600.0);
        let d = result.render_list.rects().next().unwrap();

        assert_eq!(d.rect.origin.x, 0.0);
        assert_eq!(d.rect.size.width, 120.0);
        assert_eq!(d.corner_radii.top_left, 8.0);
    }

    /// button の label はコントロールのキャプションであって本文ではないので、
    /// フラグに関係なく常に非選択。
    #[test]
    fn button_labels_are_never_selectable() {
        let result = build_tree(&button("OK"), 800.0, 600.0);
        let label = result.render_list.texts().next().unwrap();
        assert_eq!(label.content, "OK");
        assert!(label.no_select);
    }

    #[test]
    fn nested_layout() {
        let root = div()
            .w(Px(400.0))
            .h(Px(300.0))
            .bg(Color::BLACK)
            .flex_col()
            .p(Px(10.0))
            .gap(5.0)
            .children([
                div().w(Px(100.0)).h(Px(50.0)).bg(Color::WHITE),
                div().w(Px(100.0)).h(Px(50.0)).bg(Color::WHITE),
            ]);

        let result = build_tree(&root, 800.0, 600.0);
        // Parent + 2 children = 3 rects
        assert_eq!(result.render_list.rect_count(), 3);

        let rects: Vec<_> = result.render_list.rects().collect();
        // First rect is the parent
        assert!((rects[0].rect.size.width - 400.0).abs() < 0.1);
        // Children should be inside the parent with padding offset
        // Their x should be >= parent_x + padding
        let child1_x = rects[1].rect.origin.x;
        let child1_y = rects[1].rect.origin.y;
        assert!(child1_x >= 10.0 - 0.1, "child1 x={child1_x} should be >= ~10");
        assert!(child1_y >= 10.0 - 0.1, "child1 y={child1_y} should be >= ~10");
        // Second child is below first + gap
        let child2_y = rects[2].rect.origin.y;
        assert!(child2_y > child1_y + 49.0, "child2 y={child2_y} should be > child1_y + 50");
    }

    #[test]
    fn button_produces_rect_and_text() {
        let root = button("Click me")
            .accent(Color::new(0.4, 0.3, 1.0, 1.0));

        let result = build_tree(&root, 800.0, 600.0);
        assert_eq!(result.render_list.rect_count(), 1);
        assert_eq!(result.render_list.text_count(), 1);

        let text_cmd = result.render_list.texts().next().unwrap();
        assert_eq!(text_cmd.content, "Click me");
    }

    #[test]
    fn clickable_element_creates_hit_region() {
        let root = div()
            .w(Px(100.0))
            .h(Px(100.0))
            .bg(Color::WHITE)
            .on_click(|| {});

        let result = build_tree(&root, 800.0, 600.0);
        assert_eq!(result.hit_regions.len(), 1);
        assert!(result.hit_regions[0].clickable);
    }

    // G6: wants_pointer — id 付き要素の上では true、 装飾だけの背景 div や
    // 何も無い領域では false。 3D ホストアプリのカメラ操作排他の判定面。
    #[test]
    fn wants_pointer_only_over_interactive_regions() {
        let root = div()
            .w(Px(800.0))
            .h(Px(600.0))
            .bg(Color::BLACK) // decorative background — must NOT capture
            .flex_col()
            .children([
                div().id("panel").w(Px(200.0)).h(Px(100.0)).bg(Color::WHITE),
                div().w(Px(200.0)).h(Px(100.0)).bg(Color::WHITE), // no id
            ]);
        let result = build_tree(&root, 800.0, 600.0);
        assert!(result.wants_pointer(50.0, 50.0), "over id-bearing panel");
        assert!(
            !result.wants_pointer(50.0, 150.0),
            "plain div without id must not capture"
        );
        assert!(!result.wants_pointer(700.0, 500.0), "empty background");
        assert_eq!(
            result.hit_region_at(50.0, 50.0).and_then(|r| r.id.as_deref()),
            Some("panel")
        );
    }

    // G4: collapsing_section — open なら children が描画され、 closed なら
    // header だけになる。
    #[test]
    fn collapsing_section_renders_children_only_when_open() {
        let body = || vec![text("body-row").font_size(12.0).color(Color::WHITE)];
        let closed = div().w(Px(300.0)).h(Px(300.0)).child(
            crate::forms::collapsing_section(
                "sec", "Section", false, Color::WHITE, Color::BLACK, body(),
            ),
        );
        let opened = div().w(Px(300.0)).h(Px(300.0)).child(
            crate::forms::collapsing_section(
                "sec", "Section", true, Color::WHITE, Color::BLACK, body(),
            ),
        );
        let closed_texts: Vec<String> = build_tree(&closed, 800.0, 600.0)
            .render_list.texts().map(|t| t.content.clone()).collect();
        let opened_texts: Vec<String> = build_tree(&opened, 800.0, 600.0)
            .render_list.texts().map(|t| t.content.clone()).collect();
        assert!(!closed_texts.iter().any(|t| t == "body-row"));
        assert!(opened_texts.iter().any(|t| t == "body-row"));
        // Disclosure arrow flips.
        assert!(closed_texts.iter().any(|t| t == "\u{25B6}"));
        assert!(opened_texts.iter().any(|t| t == "\u{25BC}"));
    }

    #[test]
    fn percentage_sizing() {
        let root = div()
            .w_full()
            .h_full()
            .bg(Color::BLACK);

        let result = build_tree(&root, 800.0, 600.0);
        assert_eq!(result.render_list.rect_count(), 1);

        let rect_cmd = result.render_list.rects().next().unwrap();
        assert!((rect_cmd.rect.size.width - 800.0).abs() < 0.1);
        assert!((rect_cmd.rect.size.height - 600.0).abs() < 0.1);
    }
}

#[cfg(test)]
mod layout_debug {
    use super::*;
    use crate::element::*;
    use crate::Color;

    #[test]
    fn taffy_stretch_directly() {
        // Test Taffy directly to see if align_items: Stretch works
        let mut taffy: TaffyTree<()> = TaffyTree::new();

        let child = taffy.new_leaf(TaffyStyle {
            size: TaffySize {
                width: taffy::Dimension::Auto,
                height: taffy::Dimension::Length(45.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }).unwrap();

        let root = taffy.new_with_children(TaffyStyle {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Column,
            align_items: Some(taffy::AlignItems::Stretch),
            size: TaffySize {
                width: taffy::Dimension::Length(1100.0),
                height: taffy::Dimension::Length(700.0),
            },
            ..Default::default()
        }, &[child]).unwrap();

        taffy.compute_layout(root, TaffySize {
            width: AvailableSpace::Definite(1100.0),
            height: AvailableSpace::Definite(700.0),
        }).unwrap();

        let root_layout = taffy.layout(root).unwrap();
        let child_layout = taffy.layout(child).unwrap();

        println!("ROOT:  size=({}, {})", root_layout.size.width, root_layout.size.height);
        println!("CHILD: pos=({}, {}) size=({}, {})",
            child_layout.location.x, child_layout.location.y,
            child_layout.size.width, child_layout.size.height);

        assert!((child_layout.size.width - 1100.0).abs() < 1.0,
            "Expected child width ~1100, got {}", child_layout.size.width);
        assert!((child_layout.size.height - 45.0).abs() < 1.0,
            "Expected child height ~45, got {}", child_layout.size.height);
    }

    #[test]
    fn build_tree_stretch() {
        // Test via build_tree: does a child without explicit width get stretched?
        let root = div()
            .w(Px(1100.0)).h(Px(700.0))
            .bg(Color::BLACK)
            .flex_col()
            .children([
                div().h(Px(45.0)).bg(Color::WHITE),
            ]);
        let result = build_tree(&root, 1100.0, 700.0);
        let rects: Vec<_> = result.render_list.rects().collect();

        println!("rect count: {}", rects.len());
        for (i, r) in rects.iter().enumerate() {
            println!("  [{}] pos=({:.1}, {:.1}) size=({:.1}x{:.1})",
                i, r.rect.origin.x, r.rect.origin.y,
                r.rect.size.width, r.rect.size.height);
        }

        assert!(rects.len() >= 2, "Expected at least 2 rects, got {}", rects.len());
        assert!((rects[1].rect.size.width - 1100.0).abs() < 1.0,
            "Child should stretch to 1100, got {}", rects[1].rect.size.width);
    }
}

#[cfg(test)]
mod font_family_threading_tests {
    use super::*;
    use crate::element::Px;
    use crate::render_list::RenderCommand;
    use std::cell::RefCell;

    /// TextMeasure mock that records the font_family of every measure call.
    struct CapturingMeasure(RefCell<Vec<Option<String>>>);
    impl TextMeasure for CapturingMeasure {
        fn measure(
            &self,
            content: &str,
            font_size: f32,
            _bold: bool,
            _monospace: bool,
            font_family: Option<&str>,
            _max_width: Option<f32>,
            _max_lines: Option<u32>,
            _typo: Typography,
        ) -> crate::TextMetrics {
            self.0.borrow_mut().push(font_family.map(str::to_string));
            crate::TextMetrics::new(
                content.len() as f32 * font_size * 0.6,
                font_size * 1.4,
                font_size * 1.08,
            )
        }
    }

    /// `.font_family()` must survive the trip: Element → measure callback →
    /// TextDraw render command.
    #[test]
    fn font_family_reaches_measure_and_text_draw() {
        let root = crate::element::div().w(Px(200.0)).h(Px(50.0)).children([
            crate::element::text("preview").mono().font_family("HackGen"),
            crate::element::text("plain").mono(),
        ]);
        let m = CapturingMeasure(RefCell::new(Vec::new()));
        let result = build_tree_measured(&root, 200.0, 50.0, &m);

        let seen = m.0.borrow();
        assert!(
            seen.contains(&Some("HackGen".to_string())),
            "measure never saw the override: {seen:?}"
        );

        let texts: Vec<(&str, Option<&str>)> = result
            .render_list
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::Text(d) => Some((d.content.as_str(), d.font_family.as_deref())),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec![("preview", Some("HackGen")), ("plain", None)],
            "TextDraw font_family wrong"
        );
    }

    /// `.rotation()` は長らく RectDraw にしか渡っていなかった（= text に付けても
    /// 効かない）。text / button の両方で TextDraw まで届くこと、付けていない
    /// 要素は 0.0 のままであることを固定する。
    #[test]
    fn rotation_reaches_text_and_button_draws() {
        let angle = std::f32::consts::FRAC_PI_2;
        let root = crate::element::div().w(Px(400.0)).h(Px(200.0)).children([
            crate::element::text("回転注記").rotation(angle),
            crate::element::text("水平注記"),
            crate::element::button("回転ラベル").rotation(angle),
        ]);
        let result = build_tree(&root, 400.0, 200.0);

        let texts: Vec<(&str, f32)> = result
            .render_list
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::Text(d) => Some((d.content.as_str(), d.rotation)),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec![("回転注記", angle), ("水平注記", 0.0), ("回転ラベル", angle)],
            "TextDraw rotation wrong"
        );
    }
}

/// Container min-size is 0, not CSS `min-*: auto`.
///
/// See the comment at the `Div` arm of `convert_to_taffy_style`. These lock in
/// the two symptoms that `min-*: auto` produced — a scroll pane whose viewport
/// swallowed its own content, and text that refused to wrap in a flex row —
/// both of which previously needed a `min_h(0)` / `min_w(0)` on an *ancestor*
/// the author had to go find.
#[cfg(test)]
mod container_min_size_tests {
    use super::*;
    use crate::element::*;

    /// The tree from issue #60: a header plus a `grow(1.0)` row wrapping an
    /// `overflow_scroll` pane. Under `min-*: auto` the row inflated to the
    /// content height and the pane's viewport came out equal to its content,
    /// so there was nothing to scroll.
    fn scroll_tree(pane: fn(Element) -> Element, row: fn(Element) -> Element) -> Element {
        let content: Vec<Element> = (0..200)
            .map(|i| text(format!("row {i}")).font_size(13.0))
            .collect();
        div().flex_col().w_full().h_full().children(vec![
            div().h(Px(56.0)).shrink(0.0).child(text("header")),
            row(div().flex_row().w_full().grow(1.0)).children(vec![
                pane(div().flex_col().grow(1.0).overflow_scroll().id("body")).children(content),
            ]),
        ])
    }

    /// 900px window minus a 56px header — the pane should see 844, whatever
    /// combination of hints the author did or didn't write.
    #[test]
    fn scroll_pane_gets_the_leftover_height_without_any_min_hint() {
        let cases: Vec<(&str, Element)> = vec![
            ("h_full only", scroll_tree(|p| p.h_full(), |r| r)),
            ("no hints at all", scroll_tree(|p| p, |r| r)),
            ("min_h(0) on the pane", scroll_tree(|p| p.h_full().min_h(Px(0.0)), |r| r)),
            ("min_h(0) on the row", scroll_tree(|p| p.h_full(), |r| r.min_h(Px(0.0)))),
            ("fixed height row", scroll_tree(|p| p.h_full(), |r| r.h(Px(844.0)))),
        ];
        for (label, tree) in cases {
            let m = build_tree(&tree, 1200.0, 900.0).scroll_measures["body"].clone();
            assert_eq!(m.viewport_height, 844.0, "{label}: viewport should be the leftover height");
            assert!(
                m.content_height > m.viewport_height,
                "{label}: content {} must exceed viewport {} or there is nothing to scroll",
                m.content_height,
                m.viewport_height,
            );
        }
    }

    /// Deterministic measurer that actually wraps: the width is capped at the
    /// offered `max_width` and the height grows with the line count. Real apps
    /// always run with a measurer, and the `Text` arm only falls back to a
    /// natural-width minimum when there isn't one.
    struct WrappingMeasure;

    impl TextMeasure for WrappingMeasure {
        fn measure(
            &self,
            content: &str,
            font_size: f32,
            _bold: bool,
            _monospace: bool,
            _font_family: Option<&str>,
            max_width: Option<f32>,
            _max_lines: Option<u32>,
            _typo: crate::Typography,
        ) -> crate::TextMetrics {
            let natural = content.chars().count() as f32 * font_size * 0.5;
            let width = match max_width {
                Some(w) if w > 0.0 && w < natural => w,
                _ => natural,
            };
            let lines = (natural / width.max(1.0)).ceil().max(1.0);
            crate::TextMetrics {
                size: crate::Size { width, height: lines * font_size },
                baseline: font_size * 0.8,
            }
        }
    }

    /// The horizontal half of the same root: a flex row must be allowed to
    /// shrink below its text's natural width, or the text never wraps.
    #[test]
    fn text_wraps_in_a_flex_row_without_a_min_width_hint() {
        let long = "wrap ".repeat(60);
        let tree = div().flex_row().w(Px(200.0)).h(Px(400.0)).child(
            div().flex_col().grow(1.0).child(text(long).font_size(13.0)),
        );
        let build = build_tree_measured(&tree, 200.0, 400.0, &WrappingMeasure);
        let laid_out = build
            .render_list
            .texts()
            .find(|t| t.content.starts_with("wrap"))
            .expect("the text should be laid out");
        // `max_width` is the wrap constraint handed to the shaper. If the
        // column inflated to the text's natural width, this comes out far
        // wider than the row and the text renders as one long line.
        assert!(
            laid_out.max_width <= 200.0,
            "wrap width {} exceeds the 200px row, so the text never wraps",
            laid_out.max_width,
        );
    }

    /// An explicit minimum still wins — the change only reinterprets *unset*.
    #[test]
    fn an_explicit_min_is_still_honoured() {
        let tree = div()
            .flex_col()
            .w(Px(400.0))
            .h(Px(100.0))
            .child(div().id("floor").grow(1.0).min_h(Px(300.0)));
        let build = build_tree(&tree, 400.0, 400.0);
        let floor = build
            .hit_regions
            .iter()
            .find(|r| r.id.as_deref() == Some("floor"))
            .expect("the child should be laid out");
        assert_eq!(floor.rect.size.height, 300.0, "min_h(300) should beat the 100px parent");
    }
}
