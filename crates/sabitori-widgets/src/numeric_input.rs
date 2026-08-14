//! Stateful numeric input (egui `DragValue` 相当).
//!
//! Pairs with [`sabitori_core::forms::numeric_input`] (visual builder).
//! 一つのウィジェットで二つの入力モードを持つ:
//!
//! * **ドラッグ**: 値の上で水平ドラッグすると `step` × 移動px で増減。
//! * **テキスト編集**: 動かさずにクリック (press→release が slop 以内) で
//!   編集モードに入り、 [`TextInputState`] を再利用してカーソル/選択/IMEを
//!   そのまま使う。 Enter で確定 (パース + clamp)、 Escape でキャンセル。
//!
//! The app drives it from pointer / keyboard events:
//!
//! ```ignore
//! // on_pointer_move (button held):
//! if numeric.on_pointer_move(x) { /* value changed */ }
//! // on_click 相当 (press):
//! numeric.on_pointer_down(x);
//! // on_pointer_up:
//! if numeric.on_pointer_up() { /* entered edit mode → focus the element */ }
//! // on_focused_input while editing:
//! numeric.on_key(key, modifiers); numeric.on_char(ch);
//! ```

use crate::text_input::TextInputState;

/// 「クリック → 編集モード」 と判定する press→release 間の最大移動量 (px)。
/// これを超えて動いたらドラッグ扱い。
const DRAG_SLOP: f32 = 3.0;

/// State of a numeric drag-value input.
pub struct NumericInputState {
    /// Current committed value. Always within `[min, max]`.
    value: f64,
    /// Lower bound (inclusive). Default `f64::NEG_INFINITY`.
    pub min: f64,
    /// Upper bound (inclusive). Default `f64::INFINITY`.
    pub max: f64,
    /// Value change per dragged pixel. Default `1.0`.
    pub step: f64,
    /// Decimal places used for display and commit rounding. Default `0`.
    pub precision: usize,
    /// Unit label appended to the display text (e.g. `"mm"`). Not part of
    /// the edit buffer.
    pub suffix: String,
    /// Whether the widget is in text-edit mode.
    pub editing: bool,
    /// Embedded text-edit state (cursor / selection / IME), active while
    /// `editing` is true.
    pub edit: TextInputState,
    /// True while a pointer drag is in progress (between
    /// `on_pointer_down` and `on_pointer_up`).
    pub dragging: bool,
    drag_last_x: f32,
    drag_total: f32,
    /// Sub-step remainder so slow drags with precision rounding still
    /// accumulate (e.g. step 0.1 with precision 0).
    drag_accum: f64,
}

impl NumericInputState {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step: 1.0,
            precision: 0,
            suffix: String::new(),
            editing: false,
            edit: TextInputState::new(""),
            dragging: false,
            drag_last_x: 0.0,
            drag_total: 0.0,
            drag_accum: 0.0,
        }
    }

    /// Set `[min, max]` clamp range (inclusive). Clamps the current value.
    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self.value = self.value.clamp(min, max);
        self
    }

    /// Set the per-pixel drag step.
    pub fn with_step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    /// Set display / rounding precision (decimal places).
    pub fn with_precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    /// Set a unit suffix shown after the value (e.g. `"mm"`).
    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// Set the value, clamped to `[min, max]`.
    pub fn set_value(&mut self, v: f64) {
        self.value = v.clamp(self.min, self.max);
    }

    /// Formatted value without the suffix (used as the edit buffer seed).
    pub fn format_value(&self) -> String {
        format!("{:.*}", self.precision, self.value)
    }

    /// Text shown in display (non-edit) mode: formatted value + suffix.
    pub fn display_text(&self) -> String {
        if self.suffix.is_empty() {
            self.format_value()
        } else {
            format!("{} {}", self.format_value(), self.suffix)
        }
    }

    // ── Drag mode ─────────────────────────────────────────────────

    /// Pointer pressed on the widget. Starts a potential drag.
    /// No-op while editing (clicks inside the edit box move the cursor
    /// at a higher level instead).
    pub fn on_pointer_down(&mut self, x: f32) {
        if self.editing {
            return;
        }
        self.dragging = true;
        self.drag_last_x = x;
        self.drag_total = 0.0;
        self.drag_accum = 0.0;
    }

    /// Pointer moved while pressed. Returns `true` when the value changed.
    ///
    /// Movement within `DRAG_SLOP` is accumulated but **not applied** —
    /// a click with a 1–2px wobble must enter edit mode with the value
    /// untouched. Once the slop is exceeded, the accumulated delta is
    /// applied in one go so no movement is lost.
    pub fn on_pointer_move(&mut self, x: f32) -> bool {
        if !self.dragging {
            return false;
        }
        let dx = x - self.drag_last_x;
        self.drag_last_x = x;
        self.drag_total += dx.abs();
        self.drag_accum += dx as f64 * self.step;
        if self.drag_total <= DRAG_SLOP {
            return false;
        }
        let prev = self.value;
        self.value = (self.value + self.drag_accum).clamp(self.min, self.max);
        self.drag_accum = 0.0;
        (self.value - prev).abs() > f64::EPSILON
    }

    /// Pointer released. Returns `true` when this was a *click* (no
    /// movement beyond the slop) and the widget entered edit mode —
    /// the caller should focus the element so keyboard input routes here.
    pub fn on_pointer_up(&mut self) -> bool {
        if !self.dragging {
            return false;
        }
        self.dragging = false;
        if self.drag_total <= DRAG_SLOP {
            self.begin_edit();
            return true;
        }
        false
    }

    // ── Edit mode ─────────────────────────────────────────────────

    /// Enter text-edit mode seeded with the formatted value (selected,
    /// so typing replaces it — the spreadsheet/DAW convention).
    pub fn begin_edit(&mut self) {
        self.editing = true;
        self.edit.set_text(self.format_value());
        self.edit.set_focused(true);
        self.edit.with_mut(|i| i.select_all());
    }

    /// Parse the edit buffer, clamp, and leave edit mode.
    /// Returns `true` when the buffer parsed and the value was updated;
    /// `false` keeps the previous value (still leaves edit mode).
    pub fn commit_edit(&mut self) -> bool {
        self.editing = false;
        self.edit.set_focused(false);
        match self.edit.text().trim().parse::<f64>() {
            Ok(v) if v.is_finite() => {
                self.set_value(v);
                true
            }
            _ => false,
        }
    }

    /// Leave edit mode without changing the value.
    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.edit.set_focused(false);
        self.edit.with_mut(|i| i.preedit.clear());
    }

    /// Keyboard input while editing. Returns `true` if consumed.
    /// Enter commits, Escape cancels, everything else goes to the
    /// embedded [`TextInputState`].
    pub fn on_key(&mut self, key: sabitori_input::Key, modifiers: sabitori_input::Modifiers) -> bool {
        use sabitori_input::Key;
        if !self.editing {
            return false;
        }
        match key {
            Key::Enter => {
                self.commit_edit();
                true
            }
            Key::Escape => {
                self.cancel_edit();
                true
            }
            _ => self.edit.with_mut(|i| i.on_key(key, modifiers)),
        }
    }

    /// Printable character while editing. Filters to numeric characters
    /// (`0-9 . - + e E`) so stray letters don't pollute the buffer.
    pub fn on_char(&mut self, ch: char) {
        if !self.editing {
            return;
        }
        if ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E') {
            self.edit.with_mut(|i| i.on_char(ch));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_input::{Key, Modifiers};

    #[test]
    fn drag_changes_value_by_step_per_pixel() {
        let mut n = NumericInputState::new(10.0).with_step(0.5);
        n.on_pointer_down(100.0);
        assert!(n.on_pointer_move(120.0)); // +20px
        assert!((n.value() - 20.0).abs() < 1e-9);
        assert!(n.on_pointer_move(100.0)); // -20px back
        assert!((n.value() - 10.0).abs() < 1e-9);
        assert!(!n.on_pointer_up(), "real drag must not enter edit mode");
        assert!(!n.editing);
    }

    #[test]
    fn drag_clamps_to_range() {
        let mut n = NumericInputState::new(5.0).with_range(0.0, 10.0);
        n.on_pointer_down(0.0);
        n.on_pointer_move(1000.0);
        assert_eq!(n.value(), 10.0);
        n.on_pointer_move(-2000.0);
        assert_eq!(n.value(), 0.0);
    }

    #[test]
    fn sub_step_drag_accumulates() {
        let mut n = NumericInputState::new(0.0).with_step(0.1);
        n.on_pointer_down(0.0);
        for x in 1..=10 {
            n.on_pointer_move(x as f32);
        }
        // 10px * 0.1 = 1.0
        assert!((n.value() - 1.0).abs() < 1e-6, "got {}", n.value());
    }

    #[test]
    fn click_without_movement_enters_edit_mode() {
        let mut n = NumericInputState::new(42.0).with_precision(1);
        n.on_pointer_down(50.0);
        n.on_pointer_move(51.0); // within slop
        assert!(n.on_pointer_up());
        assert!(n.editing);
        assert_eq!(n.edit.text(), "42.0");
        assert!(n.edit.selection_range().is_some(), "seed text should be selected");
    }

    #[test]
    fn commit_parses_and_clamps() {
        let mut n = NumericInputState::new(5.0).with_range(0.0, 100.0);
        n.begin_edit();
        n.edit.set_text("250");
        assert!(n.commit_edit());
        assert_eq!(n.value(), 100.0);
        assert!(!n.editing);
    }

    #[test]
    fn invalid_input_keeps_previous_value() {
        let mut n = NumericInputState::new(7.0);
        n.begin_edit();
        n.edit.set_text("abc");
        assert!(!n.commit_edit());
        assert_eq!(n.value(), 7.0);
        assert!(!n.editing);
    }

    #[test]
    fn escape_cancels_edit() {
        let mut n = NumericInputState::new(3.0);
        n.begin_edit();
        n.edit.set_text("999");
        assert!(n.on_key(Key::Escape, Modifiers::default()));
        assert_eq!(n.value(), 3.0);
        assert!(!n.editing);
    }

    #[test]
    fn enter_commits_edit() {
        let mut n = NumericInputState::new(3.0).with_precision(2);
        n.begin_edit();
        n.edit.set_text("1.5");
        assert!(n.on_key(Key::Enter, Modifiers::default()));
        assert!((n.value() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn char_filter_rejects_letters() {
        let mut n = NumericInputState::new(0.0);
        n.begin_edit();
        n.edit.with_mut(|i| i.delete_selection());
        n.edit.text().clear();
        n.edit.with_mut(|i| i.cursor_pos = 0);
        for ch in "1a2b.5x".chars() {
            n.on_char(ch);
        }
        assert_eq!(n.edit.text(), "12.5");
    }

    #[test]
    fn display_text_has_precision_and_suffix() {
        let n = NumericInputState::new(1234.5678)
            .with_precision(2)
            .with_suffix("mm");
        assert_eq!(n.display_text(), "1234.57 mm");
        let bare = NumericInputState::new(3.0);
        assert_eq!(bare.display_text(), "3");
    }

    #[test]
    fn set_value_clamps() {
        let mut n = NumericInputState::new(0.0).with_range(-5.0, 5.0);
        n.set_value(99.0);
        assert_eq!(n.value(), 5.0);
        n.set_value(-99.0);
        assert_eq!(n.value(), -5.0);
    }

    #[test]
    fn pointer_down_is_ignored_while_editing() {
        let mut n = NumericInputState::new(1.0);
        n.begin_edit();
        n.on_pointer_down(0.0);
        assert!(!n.dragging);
        assert!(!n.on_pointer_move(50.0));
        assert_eq!(n.value(), 1.0);
    }
}
