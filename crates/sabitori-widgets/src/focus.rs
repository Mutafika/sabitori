//! Multi-field text-input focus management.
//!
//! [`TextInputState`] handles one field; an app with several fields on
//! screen (dialog inputs, a search box, inline rename …) also needs to
//! know **which** field the keyboard/IME currently belongs to.
//! [`FocusManager`] owns that: a `HashMap<id, TextInputState>` plus the
//! focused id, click-to-focus / click-away-to-blur, Tab cycling in
//! registration order, and routing of key / char / IME events to the
//! focused field.
//!
//! 埋め込みホスト（declarative ランナーを使わず build→GPU を直接駆動する
//! アプリ）から使う想定。 declarative ランナーは独自に focused_id を
//! 持つが、 そちらでも `on_click` → [`FocusManager::handle_press`]、
//! `on_focused_input` → `on_key` / `on_char` / `on_ime_*` で併用できる。
//!
//! ## Wiring (embedded host)
//!
//! ```ignore
//! // 構築: 登録 (一度だけ。 毎フレーム呼んでも既存状態は保持される)
//! focus.register("dlg-name", "名前を入力");
//! focus.register("dlg-material", "材質");
//!
//! // view(): 各フィールドを text_input で描く。 preedit もキャレットも
//! // ウィジェット側が面倒を見るので、 渡すのは状態とスタイルだけ。
//! if let Some(state) = focus.field("dlg-name") {
//!     text_input(ctx, "dlg-name", state, &style);
//! }
//!
//! // 左クリック押下 (hit_region_at の結果を渡す):
//! match focus.handle_press(hit_id.as_deref()) {
//!     FocusChange::Focused(_) | FocusChange::Blurred => { /* 再描画 */ }
//!     FocusChange::Unchanged => {}
//! }
//!
//! // キー押下 (focus.wants_keyboard() のときだけ呼ぶ):
//! match focus.on_key(key, modifiers) {
//!     FocusKeyResult::Submit(id) => { let text = focus.take_text(&id); /* 確定処理 */ }
//!     FocusKeyResult::Escape(id) => { focus.blur(); }
//!     FocusKeyResult::Moved(_) | FocusKeyResult::Consumed => {}
//!     FocusKeyResult::Ignored => { /* 印字可能文字は on_char へ */ }
//!     FocusKeyResult::NotFocused => { /* アプリのショートカット処理へ */ }
//! }
//!
//! // IME:
//! focus.on_ime_preedit(text, cursor);
//! focus.on_ime_commit(&text);
//!
//! // 毎フレーム: カーソル点滅
//! focus.tick(dt);
//! ```

use std::collections::HashMap;

use sabitori_input::{Key, Modifiers};

use crate::text_input::TextInputState;

/// Result of [`FocusManager::handle_press`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusChange {
    /// A registered field gained focus.
    Focused(String),
    /// The previously focused field lost focus (clicked elsewhere).
    Blurred,
    /// Nothing changed (same field, or no field was focused).
    Unchanged,
}

/// Result of [`FocusManager::on_key`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusKeyResult {
    /// No field is focused — the host should handle the key itself
    /// (shortcuts, camera, …).
    NotFocused,
    /// The focused field consumed the key (cursor move, backspace, …).
    Consumed,
    /// A field is focused but did not consume the key.
    Ignored,
    /// Enter was pressed in the field with this id. The manager does
    /// not clear or blur — the host decides (e.g. `take_text` + blur).
    Submit(String),
    /// Escape was pressed in the field with this id (outside IME
    /// composition). The manager stays focused — call [`FocusManager::blur`]
    /// if the host wants click-away semantics.
    Escape(String),
    /// Tab (or Shift+Tab) moved focus to the field with this id.
    Moved(String),
}

/// Focus + keyboard/IME routing across a set of [`TextInputState`]s.
///
/// Tab order is registration order. The `focused` flag on each
/// [`TextInputState`] is kept in sync, so visual builders can read it
/// directly.
pub struct FocusManager {
    fields: HashMap<String, TextInputState>,
    /// Registration order — doubles as the Tab order.
    order: Vec<String>,
    focused: Option<String>,
    /// When true (default), Tab / Shift+Tab cycle focus through the
    /// registered fields. Set false if the host wants Tab for itself.
    pub tab_navigation: bool,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
            order: Vec::new(),
            focused: None,
            tab_navigation: true,
        }
    }

    // ── Registration ──────────────────────────────────────────────

    /// Register a field by element id. Idempotent: calling again with an
    /// existing id keeps the current state (safe to call every frame).
    /// Returns the field state for further setup.
    pub fn register(
        &mut self,
        id: impl Into<String>,
        placeholder: impl Into<String>,
    ) -> &mut TextInputState {
        let id = id.into();
        if !self.fields.contains_key(&id) {
            self.fields
                .insert(id.clone(), TextInputState::new(placeholder));
            self.order.push(id.clone());
        }
        self.fields.get_mut(&id).expect("just inserted")
    }

    /// Remove a field (e.g. when its dialog closes). Blurs it first if
    /// focused.
    pub fn remove(&mut self, id: &str) {
        if self.focused.as_deref() == Some(id) {
            self.focused = None;
        }
        self.fields.remove(id);
        self.order.retain(|o| o != id);
    }

    pub fn contains(&self, id: &str) -> bool {
        self.fields.contains_key(id)
    }

    // ── Accessors ─────────────────────────────────────────────────

    pub fn field(&self, id: &str) -> Option<&TextInputState> {
        self.fields.get(id)
    }

    pub fn field_mut(&mut self, id: &str) -> Option<&mut TextInputState> {
        self.fields.get_mut(id)
    }

    /// Current text of a field ("" when unregistered).
    pub fn text(&self, id: &str) -> String {
        self.fields.get(id).map(|f| f.text()).unwrap_or_default()
    }

    /// Replace a field's text and move the cursor to the end.
    pub fn set_text(&mut self, id: &str, text: impl Into<String>) {
        if let Some(f) = self.fields.get(id) {
            f.set_text(text);
        }
    }

    /// Take the field's text out, leaving it empty (typical after Submit).
    pub fn take_text(&mut self, id: &str) -> String {
        match self.fields.get(id) {
            Some(f) => {
                let out = f.text();
                f.clear();
                out
            }
            None => String::new(),
        }
    }

    /// Display text for the visual builder: preedit-spliced while
    /// focused, placeholder when empty.
    pub fn display_text(&self, id: &str) -> String {
        self.fields
            .get(id)
            .map(|f| f.display_text_with_preedit())
            .unwrap_or_default()
    }

    /// Whether the field is currently showing its placeholder.
    pub fn is_placeholder(&self, id: &str) -> bool {
        self.fields.get(id).map(|f| f.is_placeholder()).unwrap_or(true)
    }

    // ── Focus ─────────────────────────────────────────────────────

    pub fn focused_id(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focused.as_deref() == Some(id)
    }

    pub fn focused_field_mut(&mut self) -> Option<&mut TextInputState> {
        let id = self.focused.clone()?;
        self.fields.get_mut(&id)
    }

    /// egui の `wants_keyboard_input()` 相当: キーイベントをこちらへ
    /// ルーティングすべきか。
    pub fn wants_keyboard(&self) -> bool {
        self.focused.is_some()
    }

    /// Focus a registered field. Returns false when `id` is unknown.
    pub fn focus(&mut self, id: &str) -> bool {
        if !self.fields.contains_key(id) {
            return false;
        }
        if let Some(prev) = self.focused.take() {
            if let Some(f) = self.fields.get(&prev) {
                f.set_focused(false);
            }
        }
        if let Some(f) = self.fields.get(id) {
            f.set_focused(true);
        }
        self.focused = Some(id.to_string());
        self.restart_blink();
        true
    }

    /// Drop focus (click-away, Escape, dialog close …).
    pub fn blur(&mut self) {
        if let Some(prev) = self.focused.take() {
            if let Some(f) = self.fields.get_mut(&prev) {
                f.set_focused(false);
            }
        }
    }

    /// Interpret a left-press hit (`hit_region_at(...).id`): a registered
    /// field's id takes focus; anything else (including `None`) blurs.
    pub fn handle_press(&mut self, hit_id: Option<&str>) -> FocusChange {
        match hit_id {
            Some(id) if self.fields.contains_key(id) => {
                if self.is_focused(id) {
                    self.restart_blink();
                    FocusChange::Unchanged
                } else {
                    self.focus(id);
                    FocusChange::Focused(id.to_string())
                }
            }
            _ => {
                if self.focused.is_some() {
                    self.blur();
                    FocusChange::Blurred
                } else {
                    FocusChange::Unchanged
                }
            }
        }
    }

    /// Move focus to the next field in Tab order (wraps). Focuses the
    /// first field when none is focused. Returns the new id.
    pub fn focus_next(&mut self) -> Option<String> {
        self.cycle(1)
    }

    /// Move focus to the previous field in Tab order (wraps).
    pub fn focus_prev(&mut self) -> Option<String> {
        self.cycle(-1)
    }

    fn cycle(&mut self, dir: isize) -> Option<String> {
        if self.order.is_empty() {
            return None;
        }
        let len = self.order.len() as isize;
        let next_idx = match &self.focused {
            Some(cur) => {
                let cur_idx = self.order.iter().position(|o| o == cur)? as isize;
                (cur_idx + dir).rem_euclid(len)
            }
            None => {
                if dir >= 0 {
                    0
                } else {
                    len - 1
                }
            }
        };
        let id = self.order[next_idx as usize].clone();
        self.focus(&id);
        Some(id)
    }

    // ── Event routing ─────────────────────────────────────────────

    /// Route a key press to the focused field. See [`FocusKeyResult`].
    /// During IME composition only Escape is consumed (cancels preedit);
    /// Enter / Tab are left to the IME.
    pub fn on_key(&mut self, key: Key, modifiers: Modifiers) -> FocusKeyResult {
        let Some(id) = self.focused.clone() else {
            return FocusKeyResult::NotFocused;
        };
        self.restart_blink();
        let Some(field) = self.fields.get_mut(&id) else {
            // Stale focus (field removed without blur) — self-heal.
            self.focused = None;
            return FocusKeyResult::NotFocused;
        };
        if field.with_mut(|i| i.on_key(key, modifiers)) {
            return FocusKeyResult::Consumed;
        }
        if field.is_composing() {
            // Mid-composition: don't interpret Enter/Tab — the IME owns them.
            return FocusKeyResult::Ignored;
        }
        match key {
            Key::Enter => FocusKeyResult::Submit(id),
            Key::Escape => FocusKeyResult::Escape(id),
            Key::Tab if self.tab_navigation => {
                let moved = if modifiers.shift {
                    self.focus_prev()
                } else {
                    self.focus_next()
                };
                match moved {
                    Some(new_id) => FocusKeyResult::Moved(new_id),
                    None => FocusKeyResult::Ignored,
                }
            }
            _ => FocusKeyResult::Ignored,
        }
    }

    /// Route a printable character to the focused field. Returns true
    /// when a field received it.
    pub fn on_char(&mut self, ch: char) -> bool {
        self.restart_blink();
        match self.focused_field_mut() {
            Some(f) => {
                f.with_mut(|i| i.on_char(ch));
                true
            }
            None => false,
        }
    }

    /// Route an IME preedit update to the focused field. Returns true
    /// when a field received it.
    pub fn on_ime_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) -> bool {
        self.restart_blink();
        match self.focused_field_mut() {
            Some(f) => {
                f.with_mut(|i| i.on_ime_preedit(text, cursor));
                true
            }
            None => false,
        }
    }

    /// Route an IME commit to the focused field. Returns true when a
    /// field received it.
    pub fn on_ime_commit(&mut self, text: &str) -> bool {
        self.restart_blink();
        match self.focused_field_mut() {
            Some(f) => {
                f.with_mut(|i| i.on_ime_commit(text));
                true
            }
            None => false,
        }
    }

    /// IME was disabled — clear any composing text.
    pub fn on_ime_disabled(&mut self) {
        if let Some(f) = self.focused_field_mut() {
            f.with_mut(|i| i.preedit.clear());
        }
    }

    // ── Cursor blink ──────────────────────────────────────────────

    /// キャレットを「見えている」状態に戻す。 打鍵やフォーカス移動のたびに呼ぶ。
    /// 入力中にキャレットが消えていると、 どこに文字が入るのか分からなくなる。
    fn restart_blink(&mut self) {
        if let Some(id) = self.focused.clone() {
            if let Some(f) = self.fields.get_mut(&id) {
                f.with_mut(|i| i.blink = 0.0);
            }
        }
    }

    /// Advance the blink phase. Call once per frame.
    ///
    /// 位相は各 [`TextInputState`] が持つ (0.4.0 で寄せた)。 以前は
    /// `FocusManager` と `TextInputState` と `TextInput` が別々に点滅を数えていて、
    /// どれを見ればいいのか決まっていなかった (issue #16)。
    pub fn tick(&mut self, dt: f32) {
        for f in self.fields.values_mut() {
            f.advance(dt);
        }
    }

    /// Whether the focused field's caret is currently visible.
    pub fn cursor_visible(&self) -> bool {
        self.focused
            .as_ref()
            .and_then(|id| self.fields.get(id))
            .is_some_and(|f| f.cursor_visible())
    }

    /// Caret visibility for a specific field (false unless focused).
    pub fn cursor_visible_for(&self, id: &str) -> bool {
        self.fields.get(id).is_some_and(|f| f.cursor_visible())
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> FocusManager {
        let mut m = FocusManager::new();
        m.register("a", "field a");
        m.register("b", "field b");
        m.register("c", "field c");
        m
    }

    #[test]
    fn press_on_field_focuses_press_away_blurs() {
        let mut m = manager();
        assert_eq!(m.handle_press(Some("a")), FocusChange::Focused("a".into()));
        assert!(m.is_focused("a"));
        assert!(m.field("a").unwrap().is_focused(), "state flag synced");
        assert!(m.wants_keyboard());

        // Same field again → unchanged.
        assert_eq!(m.handle_press(Some("a")), FocusChange::Unchanged);

        // Another registered field → focus moves.
        assert_eq!(m.handle_press(Some("b")), FocusChange::Focused("b".into()));
        assert!(!m.field("a").unwrap().is_focused(), "old field unfocused");

        // Unregistered id (a button) → blur.
        assert_eq!(m.handle_press(Some("some-button")), FocusChange::Blurred);
        assert!(!m.wants_keyboard());

        // Miss (no region) while already blurred → unchanged.
        assert_eq!(m.handle_press(None), FocusChange::Unchanged);
    }

    #[test]
    fn keys_route_to_focused_field_only() {
        let mut m = manager();
        assert_eq!(
            m.on_key(Key::Backspace, Modifiers::default()),
            FocusKeyResult::NotFocused
        );
        m.focus("a");
        assert!(m.on_char('x'));
        assert!(m.on_char('y'));
        assert_eq!(m.text("a"), "xy");
        assert_eq!(m.text("b"), "");
        assert_eq!(
            m.on_key(Key::Backspace, Modifiers::default()),
            FocusKeyResult::Consumed
        );
        assert_eq!(m.text("a"), "x");
    }

    #[test]
    fn enter_submits_escape_reports() {
        let mut m = manager();
        m.focus("b");
        m.on_char('w');
        assert_eq!(
            m.on_key(Key::Enter, Modifiers::default()),
            FocusKeyResult::Submit("b".into())
        );
        // Manager stays focused — host decides what Submit means.
        assert!(m.is_focused("b"));
        assert_eq!(m.take_text("b"), "w");
        assert_eq!(m.text("b"), "");

        assert_eq!(
            m.on_key(Key::Escape, Modifiers::default()),
            FocusKeyResult::Escape("b".into())
        );
        assert!(m.is_focused("b"), "escape does not auto-blur");
    }

    #[test]
    fn tab_cycles_in_registration_order() {
        let mut m = manager();
        m.focus("a");
        assert_eq!(
            m.on_key(Key::Tab, Modifiers::default()),
            FocusKeyResult::Moved("b".into())
        );
        assert_eq!(
            m.on_key(Key::Tab, Modifiers::default()),
            FocusKeyResult::Moved("c".into())
        );
        // Wraps.
        assert_eq!(
            m.on_key(Key::Tab, Modifiers::default()),
            FocusKeyResult::Moved("a".into())
        );
        // Shift+Tab goes backwards (wraps to the end).
        let shift = Modifiers { shift: true, ..Default::default() };
        assert_eq!(m.on_key(Key::Tab, shift), FocusKeyResult::Moved("c".into()));
    }

    #[test]
    fn tab_navigation_can_be_disabled() {
        let mut m = manager();
        m.tab_navigation = false;
        m.focus("a");
        assert_eq!(
            m.on_key(Key::Tab, Modifiers::default()),
            FocusKeyResult::Ignored
        );
        assert!(m.is_focused("a"));
    }

    #[test]
    fn ime_routes_to_focused_field() {
        let mut m = manager();
        assert!(!m.on_ime_commit("壁"), "no focus → not delivered");
        m.focus("a");
        assert!(m.on_ime_preedit("かべ".into(), Some((0, 6))));
        assert!(m.field("a").unwrap().is_composing());
        // Enter during composition must NOT submit.
        assert_eq!(
            m.on_key(Key::Enter, Modifiers::default()),
            FocusKeyResult::Ignored
        );
        m.on_ime_preedit(String::new(), None);
        assert!(m.on_ime_commit("壁"));
        assert_eq!(m.text("a"), "壁");
    }

    #[test]
    fn blur_clears_preedit_and_flag() {
        let mut m = manager();
        m.focus("a");
        m.on_ime_preedit("へん".into(), None);
        m.blur();
        assert!(!m.field("a").unwrap().is_focused());
        assert!(!m.field("a").unwrap().is_composing());
    }

    #[test]
    fn register_is_idempotent_and_remove_blurs() {
        let mut m = manager();
        m.focus("a");
        m.on_char('z');
        m.register("a", "different placeholder"); // must keep state
        assert_eq!(m.text("a"), "z");
        assert!(m.is_focused("a"));

        m.remove("a");
        assert!(!m.contains("a"));
        assert!(!m.wants_keyboard());
        // Tab order shrinks: a removed, so cycle goes b → c → b.
        m.focus("b");
        assert_eq!(
            m.on_key(Key::Tab, Modifiers::default()),
            FocusKeyResult::Moved("c".into())
        );
        assert_eq!(
            m.on_key(Key::Tab, Modifiers::default()),
            FocusKeyResult::Moved("b".into())
        );
    }

    #[test]
    fn set_text_and_display_helpers() {
        let mut m = manager();
        m.set_text("a", "wall-01");
        assert_eq!(m.text("a"), "wall-01");
        assert!(!m.is_placeholder("a"));
        assert_eq!(m.display_text("a"), "wall-01");
        assert!(m.is_placeholder("b"));
        assert_eq!(m.display_text("b"), "field b");
        assert_eq!(m.field("a").unwrap().cursor_pos(), 7, "cursor at end");
    }

    #[test]
    fn blink_ticks_and_resets_on_input() {
        let mut m = manager();
        m.focus("a");
        assert!(m.cursor_visible());
        assert!(m.cursor_visible_for("a"));
        assert!(!m.cursor_visible_for("b"));
        for _ in 0..40 {
            m.tick(1.0 / 60.0); // 0.66s → hidden phase
        }
        assert!(!m.cursor_visible());
        m.on_char('q'); // typing resets the phase
        assert!(m.cursor_visible());
    }

    #[test]
    fn focus_next_from_unfocused_starts_at_first() {
        let mut m = manager();
        assert_eq!(m.focus_next().as_deref(), Some("a"));
        m.blur();
        assert_eq!(m.focus_prev().as_deref(), Some("c"));
    }
}
