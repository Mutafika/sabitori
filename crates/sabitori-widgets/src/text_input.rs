use sabitori_core::{Color, Rect};
use sabitori_anim::{Animated, Spring};

/// IME preedit (composing) state.
#[derive(Clone, Debug, Default)]
pub struct PreeditState {
    /// The current composing text (e.g. "にほん" while typing "nihon").
    pub text: String,
    /// Byte-offset cursor range within the preedit text.
    pub cursor: Option<(usize, usize)>,
}

impl PreeditState {
    pub fn is_active(&self) -> bool {
        !self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = None;
    }
}

/// State of a text input field.
pub struct TextInputState {
    pub text: String,
    pub cursor_pos: usize,
    pub selection_start: Option<usize>,
    pub focused: bool,
    pub placeholder: String,
    /// Current IME preedit (composing) state.
    pub preedit: PreeditState,
}

impl TextInputState {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            selection_start: None,
            focused: false,
            placeholder: placeholder.into(),
            preedit: PreeditState::default(),
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor_pos, s);
        self.cursor_pos += s.len();
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            // Find the previous character boundary
            let prev = self.text[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.drain(prev..self.cursor_pos);
            self.cursor_pos = prev;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor_pos < self.text.len() {
            let next = self.text[self.cursor_pos..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_pos + i)
                .unwrap_or(self.text.len());
            self.text.drain(self.cursor_pos..next);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.text[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            self.cursor_pos = self.text[self.cursor_pos..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_pos + i)
                .unwrap_or(self.text.len());
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_pos = self.text.len();
    }

    pub fn display_text(&self) -> &str {
        if self.text.is_empty() {
            &self.placeholder
        } else {
            &self.text
        }
    }

    pub fn is_placeholder(&self) -> bool {
        self.text.is_empty() && !self.preedit.is_active()
    }

    // ── IME handling ──────────────────────────────────────────────

    /// Called when IME preedit text is updated.
    /// The preedit text is displayed with an underline at the cursor position
    /// but is **not** committed to the buffer yet.
    pub fn on_ime_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        self.preedit.text = text;
        self.preedit.cursor = cursor;
    }

    /// Called when IME commits final text.
    /// Clears preedit state and inserts the committed text into the buffer.
    pub fn on_ime_commit(&mut self, text: &str) {
        self.preedit.clear();
        self.delete_selection();
        self.insert_str(text);
    }

    // ── Keyboard handling ─────────────────────────────────────────

    /// Handle a key event. `modifiers` carries Shift/Ctrl/Alt/Meta state.
    /// Returns `true` if the event was consumed.
    pub fn on_key(&mut self, key: sabitori_input::Key, modifiers: sabitori_input::Modifiers) -> bool {
        use sabitori_input::Key;

        // If IME preedit is active, most keys should be ignored here
        // (they are handled by the platform IME). Only Escape cancels preedit.
        if self.preedit.is_active() {
            if key == Key::Escape {
                self.preedit.clear();
                return true;
            }
            // Let IME handle all other keys during composition
            return false;
        }

        let is_cmd = if cfg!(target_os = "macos") {
            modifiers.meta
        } else {
            modifiers.ctrl
        };

        match key {
            Key::Backspace => {
                if self.has_selection() {
                    self.delete_selection();
                } else {
                    self.backspace();
                }
                true
            }
            Key::Delete => {
                if self.has_selection() {
                    self.delete_selection();
                } else {
                    self.delete();
                }
                true
            }
            Key::Left => {
                if modifiers.shift {
                    self.extend_selection_left();
                } else {
                    self.collapse_selection();
                    self.move_left();
                }
                true
            }
            Key::Right => {
                if modifiers.shift {
                    self.extend_selection_right();
                } else {
                    self.collapse_selection();
                    self.move_right();
                }
                true
            }
            Key::Home => {
                if modifiers.shift {
                    self.extend_selection_to(0);
                } else {
                    self.collapse_selection();
                    self.move_home();
                }
                true
            }
            Key::End => {
                if modifiers.shift {
                    self.extend_selection_to(self.text.len());
                } else {
                    self.collapse_selection();
                    self.move_end();
                }
                true
            }
            Key::A if is_cmd => {
                self.select_all();
                true
            }
            // Cmd+C / Cmd+X / Cmd+V are typically handled at a higher level
            // (clipboard access requires platform APIs). We signal consumption
            // so the app layer knows the intent.
            Key::C if is_cmd => true,
            Key::X if is_cmd => {
                self.delete_selection();
                true
            }
            Key::V if is_cmd => {
                // The actual paste text will arrive via CharInput or ImeCommit.
                true
            }
            Key::Z if is_cmd => {
                // Undo is not yet implemented.
                false
            }
            Key::Enter | Key::Tab | Key::Escape => {
                // Bubble up to the app layer.
                false
            }
            _ => false,
        }
    }

    /// Handle a printable character input (non-IME path).
    pub fn on_char(&mut self, ch: char) {
        // Ignore during active IME composition
        if self.preedit.is_active() {
            return;
        }
        self.delete_selection();
        self.insert_char(ch);
    }

    // ── Selection helpers ─────────────────────────────────────────

    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some_and(|s| s != self.cursor_pos)
    }

    /// Returns the ordered (start, end) byte range of the selection.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_start.and_then(|s| {
            if s == self.cursor_pos {
                None
            } else {
                let lo = s.min(self.cursor_pos);
                let hi = s.max(self.cursor_pos);
                Some((lo, hi))
            }
        })
    }

    /// Delete the selected text and collapse the cursor.
    pub fn delete_selection(&mut self) {
        if let Some((lo, hi)) = self.selection_range() {
            self.text.drain(lo..hi);
            self.cursor_pos = lo;
            self.selection_start = None;
        }
    }

    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor_pos = self.text.len();
    }

    fn collapse_selection(&mut self) {
        self.selection_start = None;
    }

    fn extend_selection_left(&mut self) {
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }
        self.move_left();
    }

    fn extend_selection_right(&mut self) {
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }
        self.move_right();
    }

    fn extend_selection_to(&mut self, pos: usize) {
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }
        self.cursor_pos = pos;
    }

    // ── Display helpers ───────────────────────────────────────────

    /// Text to display, including preedit composing text spliced in at cursor.
    pub fn display_text_with_preedit(&self) -> String {
        if self.preedit.is_active() {
            let mut buf = String::with_capacity(self.text.len() + self.preedit.text.len());
            buf.push_str(&self.text[..self.cursor_pos]);
            buf.push_str(&self.preedit.text);
            buf.push_str(&self.text[self.cursor_pos..]);
            buf
        } else if self.text.is_empty() {
            self.placeholder.clone()
        } else {
            self.text.clone()
        }
    }

    /// Byte range within `display_text_with_preedit()` that should be rendered
    /// with an underline to indicate composing text. Returns `None` when no
    /// preedit is active.
    pub fn preedit_underline_range(&self) -> Option<(usize, usize)> {
        if self.preedit.is_active() {
            let start = self.cursor_pos;
            let end = start + self.preedit.text.len();
            Some((start, end))
        } else {
            None
        }
    }

    /// Standard router for a focused text field. Feed it the events the runtime
    /// delivers to `DeclarativeApp::on_focused_input` and it drives editing,
    /// caret movement and IME composition, returning whether the event was
    /// consumed. Lets apps drop the copy-pasted per-field
    /// `match event { CharInput => on_char, KeyInput => on_key, ImePreedit =>
    /// on_ime_preedit, ImeCommit => on_ime_commit, .. }`. Pair with the
    /// [`text_input`] widget for rendering.
    pub fn on_focused_input(&mut self, event: &sabitori_input::InputEvent) -> bool {
        use sabitori_input::InputEvent;
        match event {
            InputEvent::CharInput(ch) => {
                self.on_char(*ch);
                true
            }
            InputEvent::KeyInput { key, pressed: true, modifiers } => {
                self.on_key(*key, *modifiers)
            }
            InputEvent::ImePreedit { text, cursor } => {
                self.on_ime_preedit(text.clone(), *cursor);
                true
            }
            InputEvent::ImeCommit { text } => {
                self.on_ime_commit(text);
                true
            }
            _ => false,
        }
    }
}

/// Visual style for the [`text_input`] widget.
#[derive(Clone, Copy)]
pub struct TextInputStyle {
    pub bg: Color,
    pub border: Color,
    pub text: Color,
    /// Color used when the field is empty (shows its placeholder).
    pub placeholder: Color,
    pub font_size: f32,
    pub radius: f32,
    pub padding: f32,
}

/// A focusable single-line text field rendered from a [`TextInputState`].
///
/// Shows committed text plus inline IME preedit (via
/// [`TextInputState::display_text_with_preedit`]) and dims to
/// [`TextInputStyle::placeholder`] when empty. Give it a stable `id`, route keys
/// to [`TextInputState::on_focused_input`] inside
/// `DeclarativeApp::on_focused_input`, and pin focus to the same `id` from
/// `desired_focus` so the runtime enables the IME on it.
pub fn text_input(
    id: &str,
    input: &TextInputState,
    style: &TextInputStyle,
) -> sabitori_core::element::Element {
    use sabitori_core::element::{div, text};
    let color = if input.is_placeholder() {
        style.placeholder
    } else {
        style.text
    };
    div()
        .id(id)
        .focusable()
        .w_full()
        .p_px(style.padding)
        .bg(style.bg)
        .border(1.0, style.border)
        .rounded_px(style.radius)
        .child(
            text(input.display_text_with_preedit())
                .font_size(style.font_size)
                .color(color),
        )
}

/// Text input widget.
pub struct TextInput {
    pub bounds: Rect,
    pub state: TextInputState,
    pub border_anim: Animated<Color>,
    pub cursor_blink: f32,
}

impl TextInput {
    pub fn new(x: f32, y: f32, width: f32, placeholder: impl Into<String>) -> Self {
        Self {
            bounds: Rect::new(x, y, width, 40.0),
            state: TextInputState::new(placeholder),
            border_anim: Animated::new(Color::from_hex("#3a3a55"))
                .with_spring(Spring::snappy()),
            cursor_blink: 0.0,
        }
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.state.focused = focused;
        if focused {
            self.border_anim.set_target(Color::from_hex("#6c63ff"));
            self.cursor_blink = 0.0;
        } else {
            self.border_anim.set_target(Color::from_hex("#3a3a55"));
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.border_anim.tick(dt);
        if self.state.focused {
            self.cursor_blink += dt;
            if self.cursor_blink > 1.0 {
                self.cursor_blink -= 1.0;
            }
        }
    }

    pub fn cursor_visible(&self) -> bool {
        self.state.focused && self.cursor_blink < 0.5
    }
}

#[cfg(test)]
mod router_tests {
    use super::*;
    use sabitori_input::InputEvent;

    #[test]
    fn on_focused_input_routes_char_and_ime() {
        let mut s = TextInputState::new("placeholder");
        // Plain char.
        assert!(s.on_focused_input(&InputEvent::CharInput('a')));
        assert_eq!(s.text, "a");
        // IME preedit shows inline but is not committed to the buffer.
        assert!(s.on_focused_input(&InputEvent::ImePreedit {
            text: "にほん".into(),
            cursor: None,
        }));
        assert_eq!(s.display_text_with_preedit(), "aにほん");
        assert_eq!(s.text, "a");
        // Commit folds the composition in.
        assert!(s.on_focused_input(&InputEvent::ImeCommit { text: "日本".into() }));
        assert_eq!(s.text, "a日本");
    }
}
