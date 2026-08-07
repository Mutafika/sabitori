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
            // Scrollable content
            div().flex_1().h(Px(viewport_h))
                .overflow_scroll()
                .scroll_offset(0.0, scroll_y)
                .flex_col()
                .children(children),
            // Scrollbar
            scrollbar,
        ])
}

// ---------------------------------------------------------------------------
// Tooltip popup
// ---------------------------------------------------------------------------

/// Render a tooltip popup element at the given position.
///
/// The tooltip appears above the cursor position with a small offset.
/// Text width is estimated from the content length.
pub fn tooltip_popup(text_str: &str, x: f32, y: f32, bg: Color, text_color: Color, border: Color) -> Element {
    // 幅は文字数(char)基準で推定し max_w でクランプ → 超過は折返す。`len()` は byte 数で
    // JP は 3 倍になるため、長い snippet tooltip が数千 px の帯になっていた。高さは内容に追従。
    let chars = text_str.chars().count() as f32;
    let max_w = 360.0;
    let est_w = (chars * 12.5 + 20.0).min(max_w);
    div()
        .w(Px(est_w))
        .mt(Px(y + 14.0)).ml(Px(x))
        .absolute()
        .bg(bg).rounded_px(6.0).border(1.0, border)
        .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.3))
        .flex_col().p_px(8.0)
        .children([text(text_str)
            .font_size(12.0)
            .line_height(1.5)
            .color(text_color)
            .w(Px((est_w - 16.0).max(1.0)))])
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
