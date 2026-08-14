//! Form control rendering functions.
//!
//! Stateless element builders for common form controls: text inputs,
//! checkboxes, radio buttons, sliders, and dropdown triggers.
//! Same pattern as [`crate::tui`] — take values, return [`Element`].

use crate::element::{div, text, Cursor, Dimension::Px, Element};
use crate::Color;

// ---------------------------------------------------------------------------
// Text input
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Numeric input (drag-value)
// ---------------------------------------------------------------------------

/// Render a numeric drag-value input (egui `DragValue` 相当の見た目).
///
/// Pair with `sabitori_widgets::NumericInputState`, which owns the value,
/// the drag math, and the embedded text-edit state:
///
/// * `display_text` — what to show. Display mode: `state.display_text()`
///   (formatted value + suffix). Edit mode: `state.edit.display_text_with_preedit()`.
/// * `editing` — when true, renders as a text-edit box (I-beam cursor,
///   focus border); when false, renders as a draggable value (ew-resize
///   cursor).
/// * `cursor_visible` — blinking caret state, only honored while editing.
///
/// The element is focusable and carries `id` for hit-testing. Chain `.w()`
/// on the result to size it; default width is content-based.
pub fn numeric_input(
    id: &str,
    display_text: &str,
    editing: bool,
    cursor_visible: bool,
    text_color: Color,
    bg: Color,
    border_color: Color,
    focus_border_color: Color,
) -> Element {
    let active_border = if editing { focus_border_color } else { border_color };

    let mut inner_children: Vec<Element> = vec![
        text(display_text)
            .mono()
            .font_size(13.0)
            .color(text_color)
            .shrink(0.0),
    ];
    if editing && cursor_visible {
        inner_children.push(
            div()
                .w(Px(1.5))
                .h(Px(16.0))
                .bg(text_color)
                .shrink(0.0),
        );
    }

    let inner = div()
        .flex_row()
        .items_center()
        .justify_center()
        .px_pad(Px(8.0))
        .flex_1()
        .children(inner_children);

    let mut el = div()
        .h(Px(28.0))
        .bg(bg)
        .border(1.0, active_border)
        .rounded_px(5.0)
        .cursor(if editing { Cursor::Text } else { Cursor::ResizeEw })
        .id(id)
        .child(inner);
    el.focusable = true;
    el
}

// ---------------------------------------------------------------------------
// Checkbox
// ---------------------------------------------------------------------------

/// Render a checkbox with label.
///
/// * `checked` — whether the checkbox is checked (shows "✓").
pub fn checkbox(
    id: &str,
    label: &str,
    checked: bool,
    text_color: Color,
    check_color: Color,
    border_color: Color,
) -> Element {
    let box_el = if checked {
        div()
            .w(Px(18.0))
            .h(Px(18.0))
            .border(1.5, check_color)
            .rounded_px(3.0)
            .bg(check_color)
            .items_center()
            .justify_center()
            .shrink(0.0)
            .child(
                text("\u{2713}") // ✓
                    .mono()
                    .font_size(14.0)
                    .color(Color::WHITE)
                    .shrink(0.0),
            )
    } else {
        div()
            .w(Px(18.0))
            .h(Px(18.0))
            .border(1.5, border_color)
            .rounded_px(3.0)
            .bg(Color::TRANSPARENT)
            .shrink(0.0)
    };

    div()
        .flex_row()
        .gap(8.0)
        .items_center()
        .id(id)
        .children([
            box_el,
            text(label)
                .mono()
                .font_size(14.0)
                .color(text_color)
                .shrink(0.0),
        ])
}

// ---------------------------------------------------------------------------
// Collapsing header
// ---------------------------------------------------------------------------

/// Render a collapsing-section header row (egui `CollapsingHeader` 相当).
///
/// Shows a disclosure triangle (`▼` open / `▶` closed) and the title.
/// State is a plain `bool` the app owns — toggle it in `on_click` when
/// `id` is clicked. Use [`collapsing_section`] for the header + body
/// combination.
pub fn collapsing_header(
    id: &str,
    title: &str,
    open: bool,
    text_color: Color,
    bg: Color,
) -> Element {
    let arrow = if open { "\u{25BC}" } else { "\u{25B6}" }; // ▼ / ▶
    div()
        .id(id)
        .w_full()
        .h(Px(26.0))
        .bg(bg)
        .rounded_px(4.0)
        .px_pad(Px(8.0))
        .flex_row()
        .items_center()
        .gap(6.0)
        .cursor(Cursor::Pointer)
        .children([
            text(arrow).font_size(9.0).color(text_color).shrink(0.0),
            text(title).font_size(13.0).bold().color(text_color).shrink(0.0),
        ])
}

/// Render a full collapsing section: header + (children when `open`).
///
/// ```ignore
/// // app state: open_sections: HashSet<String>
/// collapsing_section(
///     "sec-wall", "壁プロパティ", self.open_sections.contains("sec-wall"),
///     t.text, t.surface,
///     vec![ /* property rows */ ],
/// )
/// // on_click: if id == "sec-wall" { toggle }
/// ```
pub fn collapsing_section(
    id: &str,
    title: &str,
    open: bool,
    text_color: Color,
    header_bg: Color,
    children: Vec<Element>,
) -> Element {
    let mut section = div()
        .w_full()
        .flex_col()
        .gap(4.0)
        .child(collapsing_header(id, title, open, text_color, header_bg));
    if open {
        section = section.child(
            div()
                .w_full()
                .flex_col()
                .gap(4.0)
                .pl(Px(14.0))
                .children(children),
        );
    }
    section
}

// ---------------------------------------------------------------------------
// Radio button
// ---------------------------------------------------------------------------

/// Render a radio button with label.
///
/// * `selected` — whether this option is currently selected (shows inner dot).
pub fn radio(
    id: &str,
    label: &str,
    selected: bool,
    text_color: Color,
    select_color: Color,
    border_color: Color,
) -> Element {
    let circle = if selected {
        div()
            .w(Px(18.0))
            .h(Px(18.0))
            .border(1.5, select_color)
            .rounded_px(9.0)
            .bg(Color::TRANSPARENT)
            .items_center()
            .justify_center()
            .shrink(0.0)
            .child(
                div()
                    .w(Px(8.0))
                    .h(Px(8.0))
                    .rounded_px(4.0)
                    .bg(select_color)
                    .shrink(0.0),
            )
    } else {
        div()
            .w(Px(18.0))
            .h(Px(18.0))
            .border(1.5, border_color)
            .rounded_px(9.0)
            .bg(Color::TRANSPARENT)
            .shrink(0.0)
    };

    div()
        .flex_row()
        .gap(8.0)
        .items_center()
        .id(id)
        .children([
            circle,
            text(label)
                .mono()
                .font_size(14.0)
                .color(text_color)
                .shrink(0.0),
        ])
}

// ---------------------------------------------------------------------------
// Slider
// ---------------------------------------------------------------------------

/// Render a horizontal slider.
///
/// * `value` — normalized value in `0.0..=1.0`.
/// * `track_w` — total track width in pixels.
pub fn slider(
    id: &str,
    value: f32,
    track_w: f32,
    track_color: Color,
    fill_color: Color,
    knob_color: Color,
) -> Element {
    let clamped = value.clamp(0.0, 1.0);
    let fill_w = clamped * track_w;
    let knob_x = clamped * (track_w - 16.0);

    let track = div()
        .w_full()
        .h(Px(4.0))
        .bg(track_color)
        .rounded_px(2.0)
        .overflow_hidden()
        .child(
            div()
                .w(Px(fill_w))
                .h(Px(4.0))
                .bg(fill_color)
                .rounded_px(2.0)
                .shrink(0.0),
        );

    let knob = div()
        .w(Px(16.0))
        .h(Px(16.0))
        .bg(knob_color)
        .rounded_px(8.0)
        .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.3))
        .pos(knob_x, 4.0)
        .shrink(0.0);

    let mut el = div()
        .w(Px(track_w))
        .h(Px(24.0))
        .id(id)
        .items_center()
        .children([track, knob]);
    el.focusable = true;
    el
}

/// Render a labeled slider row: `[label]  [track]  [value]`.
///
/// Combines a label, the visual slider, and a value readout into a single
/// flex-row. The slider track gets `id`; pair it with [`sabitori_widgets::SliderState`]
/// in your app to drive interaction.
///
/// * `id` — slider element id (used for hit-testing and focus).
/// * `label` — left-side text (typically the parameter name).
/// * `value_str` — right-side readout (e.g. `"0.42"`).
/// * `value` — normalized value in `0.0..=1.0` for the bar fill.
/// * `label_w`/`track_w`/`value_w` — fixed widths for each segment.
pub fn labeled_slider(
    id: &str,
    label: &str,
    value_str: &str,
    value: f32,
    label_w: f32,
    track_w: f32,
    value_w: f32,
    text_color: Color,
    track_color: Color,
    fill_color: Color,
    knob_color: Color,
) -> Element {
    div()
        .flex_row()
        .gap(8.0)
        .items_center()
        .h(Px(24.0))
        .children([
            text(label)
                .mono()
                .font_size(12.0)
                .color(text_color)
                .w(Px(label_w))
                .shrink(0.0),
            slider(id, value, track_w, track_color, fill_color, knob_color),
            text(value_str)
                .mono()
                .font_size(11.0)
                .color(text_color)
                .w(Px(value_w))
                .shrink(0.0),
        ])
}

// ---------------------------------------------------------------------------
// Dropdown trigger
// ---------------------------------------------------------------------------

/// Render a dropdown trigger button (closed or open state).
///
/// * `selected_label` — text displayed for the currently selected option.
/// * `open` — when true, shows "▲"; when false, shows "▼".
pub fn dropdown_trigger(
    id: &str,
    selected_label: &str,
    open: bool,
    text_color: Color,
    bg: Color,
    border_color: Color,
) -> Element {
    let arrow = if open { "\u{25B2}" } else { "\u{25BC}" }; // ▲ / ▼

    div()
        .flex_row()
        .justify_between()
        .items_center()
        .w_full()
        .h(Px(36.0))
        .bg(bg)
        .border(1.0, border_color)
        .rounded_px(6.0)
        .px_pad(Px(10.0))
        .id(id)
        .children([
            text(selected_label)
                .mono()
                .font_size(14.0)
                .color(text_color)
                .shrink(0.0),
            text(arrow)
                .font_size(10.0)
                .color(text_color)
                .shrink(0.0),
        ])
}

// ---------------------------------------------------------------------------
// Progress bar (GUI fill bar)
// ---------------------------------------------------------------------------

/// Render a GUI progress / fill-rate bar: a rounded track with a
/// proportionally filled segment. (`tui::progress_bar` のテキスト版
/// `[████░░] 80%` とは別物 — こちらは div ベースの塗りつぶしバー。
/// 占積率・進捗率などの頻出パターンの公式版。)
///
/// * `fraction` — fill ratio, clamped to `0.0..=1.0`.
/// * `height` — bar height in px (e.g. `8.0`).
///
/// The bar is `w_full`; chain `.w(Px(..))` on the result for a fixed
/// width. Pass an `id` only if the bar should capture the pointer.
pub fn progress_bar(
    fraction: f32,
    height: f32,
    track_color: Color,
    fill_color: Color,
) -> Element {
    let clamped = fraction.clamp(0.0, 1.0);
    let radius = height / 2.0;
    div()
        .w_full()
        .h(Px(height))
        .bg(track_color)
        .rounded_px(radius)
        .overflow_hidden()
        .child(
            div()
                .w(crate::element::Dimension::Percent(clamped * 100.0))
                .h(Px(height))
                .bg(fill_color)
                .rounded_px(radius)
                .shrink(0.0),
        )
}

/// Render a labeled progress row: `[label]  [bar]  [value]`.
///
/// * `value_str` — right-side readout (e.g. `"75%"`, `"32/45"`).
pub fn labeled_progress_bar(
    label: &str,
    value_str: &str,
    fraction: f32,
    label_w: f32,
    value_w: f32,
    text_color: Color,
    track_color: Color,
    fill_color: Color,
) -> Element {
    div()
        .flex_row()
        .gap(8.0)
        .items_center()
        .children([
            text(label)
                .mono()
                .font_size(12.0)
                .color(text_color)
                .w(Px(label_w))
                .shrink(0.0),
            div()
                .flex_1()
                .child(progress_bar(fraction, 8.0, track_color, fill_color)),
            text(value_str)
                .mono()
                .font_size(11.0)
                .color(text_color)
                .w(Px(value_w))
                .shrink(0.0),
        ])
}

// ---------------------------------------------------------------------------
// Segmented control
// ---------------------------------------------------------------------------

/// Render a segmented control (tab-like selector).
///
/// * `id_prefix` — prefix for segment IDs (each segment gets `"{id_prefix}-{index}"`).
/// * `options` — labels for each segment.
/// * `selected` — index of the currently selected segment.
/// * `hovered` — currently hovered element ID (from `ctx.hovered`), used for hover highlight.
/// * `bg` — background color of the overall control.
/// * `selected_bg` — background color of the selected segment.
/// * `text_color` — text color for non-selected segments.
/// * `selected_text_color` — text color for the selected segment.
pub fn segment_control(
    id_prefix: &str,
    options: &[&str],
    selected: usize,
    hovered: Option<&str>,
    bg: Color,
    selected_bg: Color,
    text_color: Color,
    selected_text_color: Color,
) -> Element {
    let segments: Vec<Element> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let seg_id = format!("{id_prefix}-{i}");
            let is_selected = i == selected;
            let is_hovered = hovered.map_or(false, |h| h == seg_id);

            let seg_bg = if is_selected {
                selected_bg
            } else if is_hovered {
                // Subtle hover highlight for non-selected segments
                Color::new(
                    selected_bg.r * 0.3 + bg.r * 0.7,
                    selected_bg.g * 0.3 + bg.g * 0.7,
                    selected_bg.b * 0.3 + bg.b * 0.7,
                    0.5,
                )
            } else {
                Color::TRANSPARENT
            };

            let seg_text = if is_selected { selected_text_color } else { text_color };

            div()
                .id(&seg_id)
                .flex_1()
                .h(Px(32.0))
                .bg(seg_bg)
                .rounded_px(4.0)
                .flex_row()
                .items_center()
                .justify_center()
                .shrink(0.0)
                .child(
                    text(*label)
                        .mono()
                        .font_size(13.0)
                        .color(seg_text)
                        .shrink(0.0),
                )
        })
        .collect();

    div()
        .flex_row()
        .gap(2.0)
        .bg(bg)
        .rounded_px(6.0)
        .p(Px(2.0))
        .items_center()
        .children(segments)
}
