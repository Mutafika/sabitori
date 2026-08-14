//! TUI-style composition helpers.
//!
//! These functions compose existing [`Element`] primitives into
//! terminal-UI-inspired components: bordered blocks, separators,
//! status bars, and keyboard shortcut hints.
//!
//! ```ignore
//! block("Files")
//!     .border_color(theme.ansi.cyan)
//!     .bg(theme.surface)
//!     .children([
//!         text("main.rs").mono().color(theme.text_primary),
//!     ])
//! ```

use crate::element::{div, text, Dimension::Px, Element};
use crate::Color;

// ---------------------------------------------------------------------------
// Block — bordered box with title
// ---------------------------------------------------------------------------

/// Create a TUI-style bordered block with a title.
pub fn block(title: impl Into<String>) -> BlockBuilder {
    BlockBuilder {
        title: title.into(),
        border_color: Color::WHITE,
        title_color: Color::WHITE,
        bg: Color::TRANSPARENT,
        corner_radius: 3.0,
        border_width: 1.0,
        padding: 8.0,
    }
}

/// Builder for a TUI-style bordered block.
pub struct BlockBuilder {
    title: String,
    border_color: Color,
    title_color: Color,
    bg: Color,
    corner_radius: f32,
    border_width: f32,
    padding: f32,
}

impl BlockBuilder {
    /// Set the border color.
    pub fn border_color(mut self, c: Color) -> Self {
        self.border_color = c;
        self
    }

    /// Set the title text color.
    pub fn title_color(mut self, c: Color) -> Self {
        self.title_color = c;
        self
    }

    /// Set the background color.
    pub fn bg(mut self, c: Color) -> Self {
        self.bg = c;
        self
    }

    /// Set corner radius (default 3.0 for subtle TUI feel).
    pub fn rounded(mut self, r: f32) -> Self {
        self.corner_radius = r;
        self
    }

    /// Set border width (default 1.0).
    pub fn border_width(mut self, w: f32) -> Self {
        self.border_width = w;
        self
    }

    /// Set inner padding (default 8.0).
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }

    /// Build the block with children.
    pub fn children<I: IntoIterator<Item = Element>>(self, children: I) -> Element {
        let mut items: Vec<Element> = Vec::new();

        // Title row
        if !self.title.is_empty() {
            items.push(
                text(&self.title)
                    .mono()
                    .bold()
                    .font_size(12.0)
                    .shrink(0.0)
                    .color(self.title_color)
                    .pb(Px(4.0)),
            );
        }

        // Content
        items.extend(children);

        div()
            .border(self.border_width, self.border_color)
            .rounded_px(self.corner_radius)
            .bg(self.bg)
            .flex_col()
            .p(Px(self.padding))
            .gap(4.0)
            .children(items)
    }
}

// ---------------------------------------------------------------------------
// Separators
// ---------------------------------------------------------------------------

/// Horizontal separator line.
pub fn hsep(color: Color) -> Element {
    div().w_full().h(Px(1.0)).bg(color)
}

/// Vertical separator line.
pub fn vsep(color: Color) -> Element {
    div().w(Px(1.0)).h_full().bg(color)
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

/// A full-width status bar container (flex row, 24px tall).
pub fn status_bar() -> Element {
    div()
        .w_full()
        .h(Px(24.0))
        .flex_row()
        .items_center()
}

/// A colored status bar segment.
pub fn status_segment(content: &str, bg: Color, fg: Color) -> Element {
    text(content)
        .mono()
        .font_size(12.0)
        .color(fg)
        .bg(bg)
        .px_pad(Px(8.0))
        .py(Px(4.0))
}

// ---------------------------------------------------------------------------
// Key hint
// ---------------------------------------------------------------------------

/// Keyboard shortcut hint like `[q] quit`.
pub fn key_hint(key: &str, label: &str, key_color: Color, label_color: Color) -> Element {
    div()
        .flex_row()
        .gap(4.0)
        .items_center()
        .children([
            text(&format!("[{key}]"))
                .mono()
                .font_size(12.0)
                .color(key_color)
                .bold(),
            text(label)
                .mono()
                .font_size(12.0)
                .color(label_color),
        ])
}

// ---------------------------------------------------------------------------
// Typewriter — text with optional blinking cursor
// ---------------------------------------------------------------------------

/// Render typewriter text with optional blinking cursor.
///
/// `visible` is the currently revealed portion of the string.
/// When `cursor_on` is true a block cursor `█` is appended.
pub fn typewriter(visible: &str, cursor_on: bool, text_color: Color, cursor_color: Color) -> Element {
    let mut children: Vec<Element> = vec![
        text(visible)
            .mono()
            .font_size(14.0)
            .color(text_color)
            .shrink(0.0),
    ];

    if cursor_on {
        children.push(
            text("\u{2588}") // █
                .mono()
                .font_size(14.0)
                .color(cursor_color)
                .shrink(0.0),
        );
    }

    div()
        .flex_row()
        .items_center()
        .shrink(0.0)
        .children(children)
}

// ---------------------------------------------------------------------------
// Spinner — animated frame + label
// ---------------------------------------------------------------------------

/// Render a spinner frame with label.
///
/// The caller is responsible for cycling `frame` through a set of spinner
/// characters (e.g. `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`). This function simply renders the
/// current frame alongside the label.
pub fn spinner(frame: &str, label: &str, frame_color: Color, label_color: Color) -> Element {
    div()
        .flex_row()
        .gap(6.0)
        .items_center()
        .shrink(0.0)
        .children([
            text(frame)
                .mono()
                .font_size(14.0)
                .color(frame_color)
                .shrink(0.0),
            text(label)
                .mono()
                .font_size(14.0)
                .color(label_color)
                .shrink(0.0),
        ])
}

// ---------------------------------------------------------------------------
// Progress bar — label [████░░░░] 80%
// ---------------------------------------------------------------------------

/// Render a text-based progress bar: `label [████░░░░] 80%`
///
/// * `filled` — number of filled cells (out of `total`).
/// * `total`  — total number of cells in the bar.
/// * `pct`    — percentage value displayed after the bar (0.0–100.0).
pub fn progress_bar(
    label: &str,
    filled: usize,
    total: usize,
    pct: f32,
    bar_color: Color,
    label_color: Color,
) -> Element {
    let filled = filled.min(total);
    let empty = total - filled;

    let bar_filled: String = "\u{2588}".repeat(filled); // █
    let bar_empty: String = "\u{2591}".repeat(empty);    // ░
    let bar_str = format!("[{bar_filled}{bar_empty}]");
    let pct_str = format!("{:.0}%", pct);

    div()
        .flex_row()
        .gap(6.0)
        .items_center()
        .shrink(0.0)
        .children([
            text(label)
                .mono()
                .font_size(12.0)
                .color(label_color)
                .shrink(0.0),
            text(&bar_str)
                .mono()
                .font_size(12.0)
                .color(bar_color)
                .shrink(0.0),
            text(&pct_str)
                .mono()
                .font_size(12.0)
                .color(label_color)
                .shrink(0.0),
        ])
}

// ---------------------------------------------------------------------------
// Gradient text — per-character coloring
// ---------------------------------------------------------------------------

/// Render per-character colored text. `color_fn` receives the char index
/// (0-based) and returns the [`Color`] for that character.
pub fn gradient_text(content: &str, color_fn: impl Fn(usize) -> Color) -> Element {
    let children: Vec<Element> = content
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            text(ch.to_string())
                .mono()
                .font_size(14.0)
                .color(color_fn(i))
                .shrink(0.0)
        })
        .collect();

    div()
        .flex_row()
        .shrink(0.0)
        .children(children)
}

// ---------------------------------------------------------------------------
// Wave text — per-character vertical offset
// ---------------------------------------------------------------------------

/// Render per-character vertically offset text. `offset_fn` returns the
/// top padding in pixels for each character index, creating a wave effect.
pub fn wave_text(content: &str, offset_fn: impl Fn(usize) -> f32, text_color: Color) -> Element {
    let children: Vec<Element> = content
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            text(ch.to_string())
                .mono()
                .font_size(14.0)
                .color(text_color)
                .shrink(0.0)
                .pt(Px(offset_fn(i)))
        })
        .collect();

    div()
        .flex_row()
        .items_end()
        .shrink(0.0)
        .children(children)
}

// ---------------------------------------------------------------------------
// Easing bar — horizontal bar for easing visualization
// ---------------------------------------------------------------------------

/// Render a horizontal bar with label for easing visualization.
///
/// * `value`     — current value (0.0–1.0).
/// * `max_width` — maximum bar width in pixels.
pub fn easing_bar(
    label: &str,
    value: f32,
    max_width: f32,
    bar_color: Color,
    label_color: Color,
) -> Element {
    let clamped = value.clamp(0.0, 1.0);
    let bar_width = clamped * max_width;

    div()
        .flex_row()
        .gap(8.0)
        .items_center()
        .shrink(0.0)
        .children([
            text(label)
                .mono()
                .font_size(12.0)
                .color(label_color)
                .shrink(0.0),
            div()
                .w(Px(max_width))
                .h(Px(12.0))
                .bg(label_color.with_alpha(0.15))
                .rounded_px(2.0)
                .shrink(0.0)
                .overflow_hidden()
                .child(
                    div()
                        .w(Px(bar_width))
                        .h(Px(12.0))
                        .bg(bar_color)
                        .rounded_px(2.0)
                        .shrink(0.0),
                ),
        ])
}

// ---------------------------------------------------------------------------
// Scroll container with scrollbar
// ---------------------------------------------------------------------------

/// A scrollable container with a visible scrollbar.
///
/// * `viewport_h` — visible height in pixels.
/// * `content_h`  — total content height in pixels.
/// * `scroll_y`   — current scroll offset.
/// * `track_color` — scrollbar track background.
/// * `thumb_color` — scrollbar thumb color.
/// * `children`   — content elements.
pub fn scroll_container(
    viewport_h: f32,
    content_h: f32,
    scroll_y: f32,
    track_color: Color,
    thumb_color: Color,
    children: Vec<Element>,
) -> Element {
    let ratio = (viewport_h / content_h.max(1.0)).min(1.0);
    let show_bar = ratio < 1.0;

    let thumb_h = (ratio * viewport_h).max(20.0);
    let max_scroll = (content_h - viewport_h).max(0.0);
    let scroll_ratio = if max_scroll > 0.0 { scroll_y / max_scroll } else { 0.0 };
    let thumb_top = scroll_ratio * (viewport_h - thumb_h);

    let scrollbar = if show_bar {
        div().w(Px(6.0)).h(Px(viewport_h)).shrink(0.0)
            .bg(track_color)
            .rounded_px(3.0)
            .overflow_hidden()
            .flex_col()
            .children([
                div().h(Px(thumb_top)).shrink(0.0),
                div().w(Px(6.0)).h(Px(thumb_h)).shrink(0.0)
                    .bg(thumb_color)
                    .rounded_px(3.0),
                div().flex_1(), // absorb remaining space
            ])
    } else {
        div()
    };

    div()
        .w_full().h(Px(viewport_h))
        .flex_row()
        .children([
            // Scrollable content.
            //
            // 呼び出し側が `scroll_y` を渡す API なので、 位置はアプリ所有
            // (`.scroll_manual`)。 以前は `.overflow_scroll().scroll_offset(..)` と
            // 書いていたが、 ランタイムが `Overflow::Scroll` の要素を全部管理対象に
            // していたため、 **渡された `scroll_y` は毎フレーム 0 に上書きされていた**
            // (issue #14)。 このコンポーネントは実質動いていなかった。
            div().flex_1().h(Px(viewport_h))
                .scroll_manual(0.0, scroll_y)
                .flex_col()
                .children(children),
            // Scrollbar
            scrollbar,
        ])
}

// ---------------------------------------------------------------------------
// Tooltip popup
// ---------------------------------------------------------------------------

/// Render a tooltip popup element for a cursor at `(x, y)`.
///
/// 既定はカーソルの**右下**。矢印カーソルはホットスポットが先端 (左上) で本体が
/// 右下へ伸びるので、真下に少しずらすだけだと矢印が箱の上辺に乗り、`p_px(8)` の
/// 内側にある**文頭が矢印の真下に隠れる** — いちばん読みたい所が読めなくなる。
/// 当たり判定ぶん (14, 20) 逃がすのが各 OS の慣行で、それに合わせてある。
///
/// `viewport_w` / `viewport_h` は窓の論理寸法。はみ出す側では位置を返す:
/// 右が足りなければ左へずらし、下が足りなければカーソルの**上**へ回す。
/// tooltip は見えなければ意味が無い部類なので、画面外に伸ばさない。
///
/// 幅と高さは内容から**推定**する (`context_menu` の `est_h` と同じ粗さ)。実測は
/// レイアウト後にしか出ないが、位置はレイアウト前に決める必要があるため。推定は
/// 多め側に倒してあるので、返す判断は早めに出る。
pub fn tooltip_popup(
    text_str: &str,
    x: f32,
    y: f32,
    viewport_w: f32,
    viewport_h: f32,
    bg: Color,
    text_color: Color,
    border: Color,
) -> Element {
    // 幅は文字数(char)基準で推定し max_w でクランプ → 超過は折返す。`len()` は byte 数で
    // JP は 3 倍になるため、長い snippet tooltip が数千 px の帯になっていた。
    let chars = text_str.chars().count() as f32;
    let max_w = 360.0;
    let est_w = (chars * 12.5 + 20.0).min(max_w);

    // 高さの推定: 折返し後の行数 × 行送り + 上下 padding。font_size 12 の
    // line_height 1.5 = 18pt、padding は上下 8 ずつ。
    let inner_w = (est_w - 16.0).max(1.0);
    let lines = (chars * 12.5 / inner_w).ceil().max(1.0);
    let est_h = lines * 18.0 + 16.0;

    // カーソルの当たり判定ぶん逃がす。
    const OFF_X: f32 = 14.0;
    const OFF_Y: f32 = 20.0;
    // 窓の縁からの余白 (context_menu と同じ 8)。
    const EDGE: f32 = 8.0;

    // 横: 右にはみ出すならカーソルの左側へ回す。それでも入らなければ縁で止める。
    let tx = if x + OFF_X + est_w + EDGE <= viewport_w {
        x + OFF_X
    } else {
        (x - OFF_X - est_w).max(EDGE).min((viewport_w - est_w - EDGE).max(0.0))
    };
    // 縦: 下にはみ出すならカーソルの上へ回す。上にも入らなければ縁で止める。
    let ty = if y + OFF_Y + est_h + EDGE <= viewport_h {
        y + OFF_Y
    } else {
        (y - OFF_Y - est_h).max(EDGE).min((viewport_h - est_h - EDGE).max(0.0))
    };

    div()
        .w(Px(est_w))
        .pos(tx, ty)
        .bg(bg).rounded_px(6.0).border(1.0, border)
        .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.3))
        .flex_col().p_px(8.0)
        .children([text(text_str)
            .font_size(12.0)
            .line_height(1.5)
            .color(text_color)
            .w(Px(inner_w))])
}

// ---------------------------------------------------------------------------
// Context menu
// ---------------------------------------------------------------------------

/// A single context menu item.
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub shortcut: String,
    pub enabled: bool,
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into(), shortcut: String::new(), enabled: true }
    }
    pub fn shortcut(mut self, s: impl Into<String>) -> Self { self.shortcut = s.into(); self }
    pub fn enabled(mut self, e: bool) -> Self { self.enabled = e; self }
}

/// A separator in a context menu.
pub fn menu_separator(border_color: Color) -> Element {
    div().h(Px(1.0)).shrink(0.0).bg(border_color)
        .mx(Px(8.0)).my(Px(2.0))
}

/// Build a context menu overlay at the given position.
///
/// Returns a full-viewport element with a backdrop (id = `backdrop_id`)
/// and the menu positioned at `(x, y)`.
///
/// Each item's `id` is used for click handling.
/// Pass `hovered` (from `ctx.hovered`) to highlight the item under the cursor.
pub fn context_menu(
    viewport_w: f32,
    viewport_h: f32,
    x: f32,
    y: f32,
    items: Vec<Element>,
    menu_w: f32,
    backdrop_id: &str,
    bg: Color,
    border: Color,
) -> Element {
    let est_h = items.len() as f32 * 28.0 + 12.0;
    let mx = x.min(viewport_w - menu_w - 8.0).max(0.0);
    let my = y.min(viewport_h - est_h - 8.0).max(0.0);

    div()
        .id(backdrop_id)
        .w(Px(viewport_w)).h(Px(viewport_h))
        .children([
            div()
                .w(Px(menu_w))
                .shrink(0.0)
                .bg(bg)
                .border(1.0, border)
                .rounded_px(8.0)
                .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
                .p_px(4.0)
                .flex_col()
                .pos(mx, my)
                .children(items),
        ])
}

/// Build a single context menu item row.
///
/// Shows label on the left, shortcut on the right.
/// Highlighted when `hovered` is true.
pub fn context_menu_item(
    item: &MenuItem,
    hovered: bool,
    text_color: Color,
    text_dim: Color,
    hover_bg: Color,
) -> Element {
    let fg = if !item.enabled { text_dim.with_alpha(0.3) }
        else if hovered { text_color }
        else { text_dim };
    let bg_c = if hovered && item.enabled { hover_bg } else { Color::TRANSPARENT };
    let id = if item.enabled { item.id.as_str() } else { "" };

    div().id(id)
        .h(Px(28.0)).shrink(0.0)
        .bg(bg_c).rounded_px(4.0)
        .flex_row().items_center().justify_between()
        .px_pad(Px(12.0))
        .children([
            text(&item.label).mono().font_size(14.0).color(fg).shrink(0.0),
            text(&item.shortcut).mono().font_size(14.0).color(text_dim.with_alpha(0.4)).shrink(0.0),
        ])
}

#[cfg(test)]
mod tooltip_tests {
    use super::*;
    use crate::build::build_tree;
    use crate::Rect;

    const VW: f32 = 800.0;
    const VH: f32 = 600.0;

    /// tooltip を実際にレイアウトして、箱の矩形を返す。
    /// runtime と同じく、窓いっぱいの overlay 根の中に置く。
    fn box_at(text_str: &str, x: f32, y: f32, vw: f32, vh: f32) -> Rect {
        let root = div().w(Px(vw)).h(Px(vh)).children([tooltip_popup(
            text_str,
            x,
            y,
            vw,
            vh,
            Color::BLACK,
            Color::WHITE,
            Color::BLACK,
        )]);
        let result = build_tree(&root, vw, vh);
        // 最初の rect は根 (bg 無しなので出ない) を除いた tooltip の箱。
        result
            .render_list
            .rects()
            .map(|d| d.rect)
            .find(|r| r.size.width > 0.0 && r.size.height > 0.0)
            .expect("tooltip の箱が出ていない")
    }

    /// 本題の回帰: 箱がカーソルの当たり判定を外していること。
    ///
    /// 矢印はホットスポットが先端で本体が右下へ伸びる。真下に少しずらすだけだと
    /// 矢印が箱の上辺に乗り、`p_px(8)` の内側にある文頭が隠れる。
    #[test]
    fn the_box_clears_the_cursor_hotspot() {
        let r = box_at("説明", 100.0, 100.0, VW, VH);
        assert!(r.origin.x > 100.0, "箱の左辺がカーソルの右にあること: {}", r.origin.x);
        assert!(r.origin.y > 100.0, "箱の上辺がカーソルの下にあること: {}", r.origin.y);
        // 文頭 (padding 8 の内側) が矢印の本体から外れていること。
        assert!(
            r.origin.x + 8.0 > 100.0 + 12.0,
            "文頭が矢印の真下にある: {}",
            r.origin.x + 8.0
        );
    }

    /// 右端でホバーしても箱が窓の外へ伸びないこと。
    /// `est_w` は最大 360pt あるので、クランプが無いと確実にはみ出す。
    #[test]
    fn the_box_stays_inside_the_right_edge() {
        let long = "これは折り返しが要るくらい長い説明文で、右端でホバーされる";
        let r = box_at(long, VW - 20.0, 100.0, VW, VH);
        assert!(
            r.origin.x + r.size.width <= VW,
            "右へはみ出した: 右辺 {} > 窓幅 {VW}",
            r.origin.x + r.size.width
        );
        assert!(r.origin.x >= 0.0, "左へ突き抜けた: {}", r.origin.x);
    }

    /// 下端でホバーしたらカーソルの上へ回すこと。
    #[test]
    fn the_box_flips_above_the_cursor_at_the_bottom_edge() {
        let cursor_y = VH - 10.0;
        let r = box_at("説明", 100.0, cursor_y, VW, VH);
        assert!(
            r.origin.y < cursor_y,
            "下端なのに下へ出している: 上辺 {} / カーソル {cursor_y}",
            r.origin.y
        );
        assert!(
            r.origin.y + r.size.height <= VH,
            "下へはみ出した: 下辺 {}",
            r.origin.y + r.size.height
        );
    }

    /// 窓が箱より小さくても、負の位置には行かないこと。
    #[test]
    fn a_viewport_too_small_still_clamps_to_the_edge() {
        let r = box_at("とても長い説明文がここに入る", 10.0, 10.0, 120.0, 60.0);
        assert!(r.origin.x >= 0.0, "x が負: {}", r.origin.x);
        assert!(r.origin.y >= 0.0, "y が負: {}", r.origin.y);
    }

    /// 余裕がある場所では素直に右下。返す判定が過敏だと、画面の真ん中でも
    /// 上に出たりして落ち着かない。
    #[test]
    fn plenty_of_room_keeps_it_below_right() {
        let r = box_at("短い", 300.0, 300.0, VW, VH);
        assert_eq!(r.origin.x, 314.0);
        assert_eq!(r.origin.y, 320.0);
    }
}
