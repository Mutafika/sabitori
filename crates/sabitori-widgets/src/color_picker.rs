//! Compact color picker — preset palette grid + RGB fine-tuning.
//!
//! 実用重視の複合ウィジェット（フル彩度ホイールではない）。 CAD の
//! レイヤー色・プロパティ色のように 「だいたいパレットから選び、 たまに
//! RGB を直接いじる」 用途向け:
//!
//! * **パレット格子** — クリックで即選択。
//! * **R / G / B** — [`NumericInputState`] を 3 つ内蔵（0–255、 ドラッグ
//!   増減 + クリックで直接入力）。
//! * **プレビュー** — 現在色のスウォッチ。
//!
//! State and visuals are split, sabitori 流: [`ColorPickerState`] owns the
//! color + the three channel inputs and interprets events by element id;
//! [`ColorPickerState::view`] builds the panel for `view()`.
//!
//! ## Wiring
//!
//! ```ignore
//! // view():
//! picker.view(hovered, &ColorPickerStyle::default_dark())
//!
//! // on_click (swatches):
//! if let Some(color) = picker.handle_click(id) { /* apply */ }
//!
//! // pointer protocol (RGB drag — NumericInput と同じ):
//! picker.on_pointer_down(id, x);          // press 時
//! if picker.on_pointer_move(x) { /* color changed */ }
//! if let Some(edit_id) = picker.on_pointer_up() { /* focus edit_id */ }
//!
//! // keyboard while editing a channel:
//! picker.on_key(key, modifiers); picker.on_char(ch);
//! ```

use sabitori_core::element::{div, text, Element, Px};
use sabitori_core::forms::numeric_input;
use sabitori_core::Color;

use crate::numeric_input::NumericInputState;

/// Visual parameters for [`ColorPickerState::view`].
#[derive(Clone, Debug)]
pub struct ColorPickerStyle {
    pub bg: Color,
    pub border: Color,
    pub text: Color,
    pub label: Color,
    pub focus_border: Color,
    /// Border drawn around the hovered / selected swatch.
    pub swatch_highlight: Color,
    pub swatch_size: f32,
    pub swatch_gap: f32,
    /// Swatches per palette row.
    pub columns: usize,
}

impl ColorPickerStyle {
    pub fn default_dark() -> Self {
        Self {
            bg: Color::from_hex("#22223a"),
            border: Color::from_hex("#3a3a55"),
            text: Color::from_hex("#e8e8f0"),
            label: Color::from_hex("#9090a8"),
            focus_border: Color::from_hex("#6c63ff"),
            swatch_highlight: Color::from_hex("#ffffff"),
            swatch_size: 20.0,
            swatch_gap: 4.0,
            columns: 8,
        }
    }
}

/// Which RGB channel an element id refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Channel {
    R,
    G,
    B,
}

/// State for the palette + RGB color picker.
pub struct ColorPickerState {
    /// Element-id prefix; all internal ids are `"{prefix}:…"`.
    prefix: String,
    color: Color,
    pub r: NumericInputState,
    pub g: NumericInputState,
    pub b: NumericInputState,
    palette: Vec<Color>,
}

impl ColorPickerState {
    /// `id_prefix` namespaces the element ids (`"{prefix}:swatch:3"`,
    /// `"{prefix}:r"` …) so several pickers can coexist.
    pub fn new(id_prefix: impl Into<String>, initial: Color) -> Self {
        let mk = |v: f64| {
            NumericInputState::new(v)
                .with_range(0.0, 255.0)
                .with_step(1.0)
        };
        let (r, g, b, _) = initial.to_srgb8();
        Self {
            prefix: id_prefix.into(),
            color: initial,
            r: mk(r as f64),
            g: mk(g as f64),
            b: mk(b as f64),
            palette: Self::default_palette(),
        }
    }

    /// Replace the preset palette.
    pub fn with_palette(mut self, palette: Vec<Color>) -> Self {
        self.palette = palette;
        self
    }

    /// 16-color default palette: grayscale row + common hues.
    pub fn default_palette() -> Vec<Color> {
        [
            "#ffffff", "#c0c0c8", "#808090", "#404050", "#000000", "#7f3f00",
            "#ff8000", "#ffd700", "#ff4040", "#ff80c0", "#c040ff", "#4040ff",
            "#40c0ff", "#40e0d0", "#40c040", "#a0e040",
        ]
        .iter()
        .map(|h| Color::from_hex(h))
        .collect()
    }

    pub fn color(&self) -> Color {
        self.color
    }

    /// Set the color and sync the RGB inputs (e.g. when the host's
    /// selection changes). Alpha is preserved as given.
    pub fn set_color(&mut self, c: Color) {
        self.color = c;
        self.r.cancel_edit();
        self.g.cancel_edit();
        self.b.cancel_edit();
        let (r, g, b, _) = c.to_srgb8();
        self.r.set_value(r as f64);
        self.g.set_value(g as f64);
        self.b.set_value(b as f64);
    }

    /// Whether any RGB channel is in text-edit mode (keyboard should
    /// route to [`ColorPickerState::on_key`] / `on_char`).
    pub fn wants_keyboard(&self) -> bool {
        self.r.editing || self.g.editing || self.b.editing
    }

    // ── Element ids ───────────────────────────────────────────────

    fn swatch_id(&self, idx: usize) -> String {
        format!("{}:swatch:{idx}", self.prefix)
    }

    fn channel_id(&self, ch: Channel) -> String {
        let suffix = match ch {
            Channel::R => "r",
            Channel::G => "g",
            Channel::B => "b",
        };
        format!("{}:{suffix}", self.prefix)
    }

    fn channel_of(&self, id: &str) -> Option<Channel> {
        let rest = id.strip_prefix(self.prefix.as_str())?;
        match rest {
            ":r" => Some(Channel::R),
            ":g" => Some(Channel::G),
            ":b" => Some(Channel::B),
            _ => None,
        }
    }

    fn channel_state_mut(&mut self, ch: Channel) -> &mut NumericInputState {
        match ch {
            Channel::R => &mut self.r,
            Channel::G => &mut self.g,
            Channel::B => &mut self.b,
        }
    }

    // ── Event handling ────────────────────────────────────────────

    /// Interpret a click on `id`. Returns the new color when a palette
    /// swatch was picked; `None` for unrelated ids.
    pub fn handle_click(&mut self, id: &str) -> Option<Color> {
        let rest = id.strip_prefix(self.prefix.as_str())?;
        let idx: usize = rest.strip_prefix(":swatch:")?.parse().ok()?;
        let c = *self.palette.get(idx)?;
        // Keep the current alpha; palettes pick the RGB only.
        let next = Color::new(c.r, c.g, c.b, self.color.a);
        self.set_color(next);
        Some(self.color)
    }

    /// Pointer press at logical x on element `id`. Returns true when an
    /// RGB channel grabbed the pointer (starts a potential drag).
    /// Pending edits on the *other* channels are committed.
    pub fn on_pointer_down(&mut self, id: &str, x: f32) -> bool {
        let Some(ch) = self.channel_of(id) else {
            // Click elsewhere: commit any in-progress channel edit.
            self.commit_all_edits();
            return false;
        };
        for other in [Channel::R, Channel::G, Channel::B] {
            if other != ch && self.channel_state_mut(other).editing {
                self.channel_state_mut(other).commit_edit();
            }
        }
        self.sync_color_from_channels();
        self.channel_state_mut(ch).on_pointer_down(x);
        true
    }

    /// Pointer moved while pressed. Returns true when the color changed
    /// (a channel is being dragged).
    pub fn on_pointer_move(&mut self, x: f32) -> bool {
        let changed = self.r.on_pointer_move(x)
            | self.g.on_pointer_move(x)
            | self.b.on_pointer_move(x);
        if changed {
            self.sync_color_from_channels();
        }
        changed
    }

    /// Pointer released. When a click (not a drag) put a channel into
    /// edit mode, returns that channel's element id — the host should
    /// focus it so keyboard input routes here.
    pub fn on_pointer_up(&mut self) -> Option<String> {
        for ch in [Channel::R, Channel::G, Channel::B] {
            if self.channel_state_mut(ch).on_pointer_up() {
                return Some(self.channel_id(ch));
            }
        }
        None
    }

    /// Keyboard input while a channel is editing. Enter/Escape are
    /// handled by the channel (commit / cancel); the color re-syncs on
    /// commit. Returns true if consumed.
    pub fn on_key(&mut self, key: sabitori_input::Key, modifiers: sabitori_input::Modifiers) -> bool {
        for ch in [Channel::R, Channel::G, Channel::B] {
            if self.channel_state_mut(ch).editing {
                let consumed = self.channel_state_mut(ch).on_key(key, modifiers);
                if consumed && !self.channel_state_mut(ch).editing {
                    // Edit just ended (commit or cancel) → re-sync.
                    self.sync_color_from_channels();
                }
                return consumed;
            }
        }
        false
    }

    /// Printable character while a channel is editing.
    pub fn on_char(&mut self, ch: char) {
        for c in [Channel::R, Channel::G, Channel::B] {
            if self.channel_state_mut(c).editing {
                self.channel_state_mut(c).on_char(ch);
                return;
            }
        }
    }

    fn commit_all_edits(&mut self) {
        let mut any = false;
        for ch in [Channel::R, Channel::G, Channel::B] {
            if self.channel_state_mut(ch).editing {
                self.channel_state_mut(ch).commit_edit();
                any = true;
            }
        }
        if any {
            self.sync_color_from_channels();
        }
    }

    fn sync_color_from_channels(&mut self) {
        self.color = Color::from_srgb8(
            self.r.value().round() as u8,
            self.g.value().round() as u8,
            self.b.value().round() as u8,
            255,
        )
        .with_alpha(self.color.a);
    }

    // ── Element builder ───────────────────────────────────────────

    /// Build the picker panel: preview row, palette grid, RGB rows.
    pub fn view(&self, hovered: Option<&str>, style: &ColorPickerStyle) -> Element {
        let grid_w = style.columns as f32 * (style.swatch_size + style.swatch_gap)
            - style.swatch_gap;

        // Preview row.
        let preview = div()
            .flex_row()
            .items_center()
            .gap(8.0)
            .children([
                div()
                    .w(Px(28.0))
                    .h(Px(28.0))
                    .bg(self.color)
                    .border(1.0, style.border)
                    .rounded_px(4.0)
                    .shrink(0.0),
                text(&{
                    let (r, g, b, _) = self.color.to_srgb8();
                    format!("RGB({r}, {g}, {b})")
                })
                .mono()
                .font_size(11.0)
                .color(style.label)
                .shrink(0.0),
            ]);

        // Palette grid (rows of `columns`).
        let mut rows: Vec<Element> = Vec::new();
        for chunk in self.palette.chunks(style.columns.max(1)) {
            let offset = rows.len() * style.columns.max(1);
            let cells: Vec<Element> = chunk
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let id = self.swatch_id(offset + i);
                    let selected = approx_rgb(*c, self.color);
                    let is_hovered = hovered == Some(id.as_str());
                    let border = if selected || is_hovered {
                        style.swatch_highlight
                    } else {
                        style.border
                    };
                    div()
                        .id(&id)
                        .w(Px(style.swatch_size))
                        .h(Px(style.swatch_size))
                        .bg(*c)
                        .border(if selected { 2.0 } else { 1.0 }, border)
                        .rounded_px(3.0)
                        .shrink(0.0)
                })
                .collect();
            rows.push(
                div()
                    .flex_row()
                    .gap(style.swatch_gap)
                    .children(cells),
            );
        }
        let grid = div()
            .flex_col()
            .gap(style.swatch_gap)
            .w(Px(grid_w))
            .children(rows);

        // RGB rows.
        let rgb_rows: Vec<Element> = [
            ("R", Channel::R, &self.r),
            ("G", Channel::G, &self.g),
            ("B", Channel::B, &self.b),
        ]
        .into_iter()
        .map(|(label, ch, state)| {
            let display = if state.editing {
                state.edit.display_text_with_preedit()
            } else {
                state.display_text()
            };
            div()
                .flex_row()
                .items_center()
                .gap(6.0)
                .children([
                    text(label)
                        .mono()
                        .font_size(11.0)
                        .color(style.label)
                        .w(Px(14.0))
                        .shrink(0.0),
                    numeric_input(
                        &self.channel_id(ch),
                        &display,
                        state.editing,
                        state.editing,
                        style.text,
                        style.bg,
                        style.border,
                        style.focus_border,
                    )
                    .w(Px(64.0)),
                ])
        })
        .collect();

        div()
            .flex_col()
            .gap(8.0)
            .p(Px(10.0))
            .bg(style.bg)
            .border(1.0, style.border)
            .rounded_px(6.0)
            .children([preview, grid, div().flex_col().gap(4.0).children(rgb_rows)])
    }
}

/// RGB equality at sRGB 8-bit precision (alpha ignored).
fn approx_rgb(a: Color, b: Color) -> bool {
    let (ar, ag, ab, _) = a.to_srgb8();
    let (br, bg, bb, _) = b.to_srgb8();
    (ar, ag, ab) == (br, bg, bb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_input::{Key, Modifiers};

    fn picker() -> ColorPickerState {
        ColorPickerState::new("pick", Color::from_hex("#808090"))
    }

    #[test]
    fn initial_channels_match_color() {
        let p = picker();
        assert_eq!(p.r.value(), 128.0);
        assert_eq!(p.g.value(), 128.0);
        assert_eq!(p.b.value(), 144.0);
    }

    #[test]
    fn swatch_click_sets_color_and_syncs_channels() {
        let mut p = picker();
        // Palette index 4 is black in the default palette.
        let c = p.handle_click("pick:swatch:4").expect("swatch hit");
        let (r, g, b, _) = c.to_srgb8();
        assert_eq!((r, g, b), (0, 0, 0));
        assert_eq!(p.r.value(), 0.0);
        assert_eq!(p.b.value(), 0.0);
        // Unrelated ids are ignored.
        assert!(p.handle_click("other-button").is_none());
        assert!(p.handle_click("pick:swatch:999").is_none());
    }

    #[test]
    fn swatch_click_preserves_alpha() {
        let mut p = ColorPickerState::new("pick", Color::new(1.0, 0.0, 0.0, 0.5));
        let c = p.handle_click("pick:swatch:0").unwrap();
        assert_eq!(c.a, 0.5);
    }

    #[test]
    fn drag_channel_changes_color() {
        let mut p = picker();
        assert!(p.on_pointer_down("pick:r", 100.0));
        assert!(p.on_pointer_move(150.0)); // +50px * step1 → 178
        assert_eq!(p.r.value(), 178.0);
        assert_eq!(p.color().to_srgb8().0, 178);
        assert!(p.on_pointer_up().is_none(), "drag must not enter edit mode");
    }

    #[test]
    fn click_channel_enters_edit_and_commit_updates_color() {
        let mut p = picker();
        p.on_pointer_down("pick:g", 50.0);
        let focus_id = p.on_pointer_up();
        assert_eq!(focus_id.as_deref(), Some("pick:g"));
        assert!(p.wants_keyboard());

        // Type a new value (seed text is selected → replaced).
        p.on_char('2');
        p.on_char('5');
        p.on_char('5');
        assert!(p.on_key(Key::Enter, Modifiers::default()));
        assert!(!p.wants_keyboard());
        assert_eq!(p.g.value(), 255.0);
        assert!((p.color().g - 1.0).abs() < 1e-5);
    }

    #[test]
    fn escape_cancels_channel_edit() {
        let mut p = picker();
        p.on_pointer_down("pick:b", 10.0);
        p.on_pointer_up();
        p.on_char('9');
        assert!(p.on_key(Key::Escape, Modifiers::default()));
        assert_eq!(p.b.value(), 144.0, "value unchanged after cancel");
    }

    #[test]
    fn pointer_down_elsewhere_commits_pending_edit() {
        let mut p = picker();
        p.on_pointer_down("pick:r", 10.0);
        p.on_pointer_up(); // edit mode
        p.r.edit.text = "10".into();
        assert!(!p.on_pointer_down("some-other-element", 0.0));
        assert_eq!(p.r.value(), 10.0, "edit committed on click-away");
        assert_eq!(p.color().to_srgb8().0, 10);
    }

    #[test]
    fn set_color_resyncs_channels_and_cancels_edits() {
        let mut p = picker();
        p.on_pointer_down("pick:r", 10.0);
        p.on_pointer_up();
        p.set_color(Color::from_hex("#ff8000"));
        assert!(!p.wants_keyboard());
        assert_eq!(p.r.value(), 255.0);
        assert_eq!(p.g.value(), 128.0);
        assert_eq!(p.b.value(), 0.0);
    }

    #[test]
    fn view_contains_swatch_and_channel_ids() {
        let p = picker();
        let el = p.view(None, &ColorPickerStyle::default_dark());
        let mut ids = Vec::new();
        collect_ids(&el, &mut ids);
        assert!(ids.iter().any(|i| i == "pick:swatch:0"));
        assert!(ids.iter().any(|i| i == "pick:swatch:15"));
        assert!(ids.iter().any(|i| i == "pick:r"));
        assert!(ids.iter().any(|i| i == "pick:g"));
        assert!(ids.iter().any(|i| i == "pick:b"));
    }

    fn collect_ids(el: &Element, out: &mut Vec<String>) {
        if let Some(ref id) = el.id {
            out.push(id.clone());
        }
        for c in &el.children {
            collect_ids(c, out);
        }
    }
}
