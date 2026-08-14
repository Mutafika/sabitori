//! Element-id driven dropdown / select (egui `ComboBox` 相当).
//!
//! 0.4.0 より前は retained 版の `Dropdown` が並立していた。 あれは画面座標
//! `Rect` を自前で持つので、 declarative ツリーや埋め込みホスト（bamiri 等の
//! build→GPU 直接駆動）からは座標管理が二重になり使えず、 repo 内の使用箇所も
//! 0 だったため削除した。 [`DropdownState`] は [`MenuBarState`](crate::MenuBarState) と同じ
//! 「state ↔ visuals 分離 + element id でイベント解釈」 方式の dropdown:
//!
//! * [`DropdownState::trigger`] — 常設のトリガーボタン（選択中ラベル + ▼）。
//! * [`DropdownState::menu_inline`] — トリガー直下にレイアウトフローで
//!   展開する簡易メニュー（位置計算不要、 declarative アプリ向け）。
//! * [`DropdownState::overlay_at`] — `BuildResult::region_rect(trigger_id)`
//!   で得たアンカー矩形に重ねる絶対配置オーバーレイ + 全画面バックドロップ
//!   （埋め込みホスト向け。 下端で収まらなければ上に開く）。
//! * [`DropdownState::handle_click`] — クリック id の解釈
//!   （トグル / 項目選択 / バックドロップで閉じる）。
//!
//! ## Wiring (embedded host)
//!
//! ```ignore
//! // view():
//! state.trigger(&DropdownStyle::default_dark(), hovered)
//! // overlay 層 (メニューバーと同じ別 submit のレイヤー):
//! if let Some(rect) = build.region_rect(state.trigger_id()) {
//!     state.overlay_at(rect, vw, vh, hovered, &style)
//! }
//! // on_click:
//! match state.handle_click(id) {
//!     DropdownEvent::Selected(i) => { /* apply items[i] */ }
//!     DropdownEvent::Opened | DropdownEvent::Closed => { /* 再描画 */ }
//!     DropdownEvent::Ignored => { /* 他のウィジェットへ */ }
//! }
//! ```

use sabitori_core::element::{div, text, Element, Px, Role};
use sabitori_core::forms::dropdown_trigger;
use sabitori_core::{Color, Rect};

/// Visuals for [`DropdownState`]. 0.4.0 より前は retained 版 `Dropdown` と
/// 同居していたが、 そちらは削除したのでここに移した。
#[derive(Clone, Debug)]
pub struct DropdownStyle {
    pub bg: Color,
    pub bg_hover: Color,
    pub fg: Color,
    pub fg_selected: Color,
    pub border_color: Color,
    pub border_active: Color,
    pub menu_bg: Color,
    pub menu_item_hover: Color,
    pub height: f32,
    pub item_height: f32,
    pub corner_radius: f32,
    pub padding_x: f32,
    pub max_visible_items: usize,
}

impl DropdownStyle {
    pub fn default_dark() -> Self {
        Self {
            bg: Color::from_hex("#1e1e2e"),
            bg_hover: Color::from_hex("#24243a"),
            fg: Color::from_hex("#c8c8dc"),
            fg_selected: Color::from_hex("#ffffff"),
            border_color: Color::from_hex("#3a3a55"),
            border_active: Color::from_hex("#6c8cff"),
            menu_bg: Color::from_hex("#1a1a2e"),
            menu_item_hover: Color::from_hex("#2a2a48"),
            height: 32.0,
            item_height: 30.0,
            corner_radius: 6.0,
            padding_x: 10.0,
            max_visible_items: 8,
        }
    }
}

/// Result of [`DropdownState::handle_click`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropdownEvent {
    /// The menu opened.
    Opened,
    /// The menu closed without a selection (toggle / backdrop).
    Closed,
    /// Item at this index was selected (menu closed).
    Selected(usize),
    /// The id is unrelated to this dropdown.
    Ignored,
}

/// State for an element-id driven dropdown.
pub struct DropdownState {
    /// Trigger element id; menu items are `"{id}::item:{idx}"`,
    /// the overlay backdrop is `"{id}::backdrop"`.
    id: String,
    pub items: Vec<String>,
    pub selected: usize,
    pub open: bool,
}

impl DropdownState {
    pub fn new(id: impl Into<String>, items: Vec<String>) -> Self {
        Self {
            id: id.into(),
            items,
            selected: 0,
            open: false,
        }
    }

    pub fn with_selected(mut self, idx: usize) -> Self {
        self.selected = idx.min(self.items.len().saturating_sub(1));
        self
    }

    /// The trigger element id (pass to `BuildResult::region_rect` for
    /// overlay anchoring).
    pub fn trigger_id(&self) -> &str {
        &self.id
    }

    pub fn selected_label(&self) -> &str {
        self.items
            .get(self.selected)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    fn item_id(&self, idx: usize) -> String {
        format!("{}::item:{idx}", self.id)
    }

    fn backdrop_id(&self) -> String {
        format!("{}::backdrop", self.id)
    }

    // ── Event handling ────────────────────────────────────────────

    /// Interpret a click by element id. See [`DropdownEvent`].
    pub fn handle_click(&mut self, clicked_id: &str) -> DropdownEvent {
        if clicked_id == self.id {
            self.open = !self.open;
            return if self.open {
                DropdownEvent::Opened
            } else {
                DropdownEvent::Closed
            };
        }
        if clicked_id == self.backdrop_id() {
            self.open = false;
            return DropdownEvent::Closed;
        }
        if let Some(rest) = clicked_id.strip_prefix(self.id.as_str()) {
            if let Some(idx_str) = rest.strip_prefix("::item:") {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if self.open && idx < self.items.len() {
                        self.selected = idx;
                        self.open = false;
                        return DropdownEvent::Selected(idx);
                    }
                }
            }
        }
        DropdownEvent::Ignored
    }

    // ── Element builders ──────────────────────────────────────────

    /// The always-visible trigger button.
    pub fn trigger(&self, style: &DropdownStyle, hovered: Option<&str>) -> Element {
        let bg = if hovered == Some(self.id.as_str()) {
            style.bg_hover
        } else {
            style.bg
        };
        let border = if self.open {
            style.border_active
        } else {
            style.border_color
        };
        dropdown_trigger(&self.id, self.selected_label(), self.open, style.fg, bg, border)
    }

    /// Inline menu: place directly after [`DropdownState::trigger`] in a
    /// column — it expands in layout flow (content below is pushed down).
    /// Returns `None` while closed. 位置計算が要らないので、 アンカー矩形を
    /// 知らない declarative アプリでも使える。
    pub fn menu_inline(&self, hovered: Option<&str>, style: &DropdownStyle) -> Option<Element> {
        if !self.open {
            return None;
        }
        Some(self.menu_panel(hovered, style).w_full())
    }

    /// Overlay menu anchored to the trigger's screen rect (from
    /// `BuildResult::region_rect(trigger_id)`). Includes a full-viewport
    /// backdrop that closes the menu on outside clicks. Opens downward;
    /// flips above the trigger when it would overflow `viewport_h`.
    pub fn overlay_at(
        &self,
        anchor: Rect,
        viewport_w: f32,
        viewport_h: f32,
        hovered: Option<&str>,
        style: &DropdownStyle,
    ) -> Option<Element> {
        if !self.open {
            return None;
        }
        let visible = self.items.len().min(style.max_visible_items);
        let menu_h = visible as f32 * style.item_height + 2.0; // + border
        let below_y = anchor.origin.y + anchor.size.height + 2.0;
        let y = if below_y + menu_h <= viewport_h {
            below_y
        } else {
            (anchor.origin.y - menu_h - 2.0).max(0.0)
        };
        let menu = self
            .menu_panel(hovered, style)
            .w(Px(anchor.size.width))
            .pos(anchor.origin.x, y);

        Some(
            div()
                .id(&self.backdrop_id())
                .w(Px(viewport_w))
                .h(Px(viewport_h))
                .pos(0.0, 0.0)
                .overlay()
                .child(menu),
        )
    }

    /// The shared menu panel (item rows).
    fn menu_panel(&self, hovered: Option<&str>, style: &DropdownStyle) -> Element {
        let rows: Vec<Element> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                let id = self.item_id(idx);
                let bg = if hovered == Some(id.as_str()) {
                    style.menu_item_hover
                } else {
                    Color::TRANSPARENT
                };
                let fg = if idx == self.selected {
                    style.fg_selected
                } else {
                    style.fg
                };
                let mut row = div()
                    .id(&id)
                    .role(Role::ListItem)
                    .label(label)
                    .w_full()
                    .h(Px(style.item_height))
                    .bg(bg)
                    .px_pad(Px(style.padding_x))
                    .flex_row()
                    .items_center()
                    .gap(6.0)
                    .child(text(label).font_size(13.0).color(fg).shrink(0.0));
                if idx == self.selected {
                    row = row.child(text("\u{2713}").font_size(11.0).color(fg).shrink(0.0));
                }
                row
            })
            .collect();

        div()
            .bg(style.menu_bg)
            .border(1.0, style.border_color)
            .rounded_px(style.corner_radius)
            .flex_col()
            .children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dd() -> DropdownState {
        DropdownState::new(
            "dd-struct",
            vec!["木造".into(), "鉄骨造".into(), "RC造".into()],
        )
    }

    #[test]
    fn trigger_click_toggles() {
        let mut d = dd();
        assert_eq!(d.handle_click("dd-struct"), DropdownEvent::Opened);
        assert!(d.open);
        assert_eq!(d.handle_click("dd-struct"), DropdownEvent::Closed);
        assert!(!d.open);
    }

    #[test]
    fn item_click_selects_and_closes() {
        let mut d = dd();
        d.handle_click("dd-struct");
        assert_eq!(d.handle_click("dd-struct::item:2"), DropdownEvent::Selected(2));
        assert_eq!(d.selected, 2);
        assert_eq!(d.selected_label(), "RC造");
        assert!(!d.open);
    }

    #[test]
    fn item_click_while_closed_is_ignored() {
        let mut d = dd();
        assert_eq!(d.handle_click("dd-struct::item:1"), DropdownEvent::Ignored);
        assert_eq!(d.selected, 0);
    }

    #[test]
    fn backdrop_closes_without_selection() {
        let mut d = dd();
        d.handle_click("dd-struct");
        assert_eq!(d.handle_click("dd-struct::backdrop"), DropdownEvent::Closed);
        assert_eq!(d.selected, 0);
    }

    #[test]
    fn unrelated_and_out_of_range_ids_ignored() {
        let mut d = dd();
        d.handle_click("dd-struct");
        assert_eq!(d.handle_click("other"), DropdownEvent::Ignored);
        assert_eq!(d.handle_click("dd-struct::item:99"), DropdownEvent::Ignored);
        assert!(d.open, "ignored clicks must not close the menu");
    }

    #[test]
    fn inline_menu_only_when_open() {
        let style = DropdownStyle::default_dark();
        let mut d = dd();
        assert!(d.menu_inline(None, &style).is_none());
        d.handle_click("dd-struct");
        let menu = d.menu_inline(None, &style).expect("open menu");
        let ids = collect_ids(&menu);
        assert!(ids.iter().any(|i| i == "dd-struct::item:0"));
        assert!(ids.iter().any(|i| i == "dd-struct::item:2"));
    }

    #[test]
    fn overlay_opens_below_and_flips_above_near_bottom() {
        let style = DropdownStyle::default_dark();
        let mut d = dd();
        d.handle_click("dd-struct");

        // Plenty of room → below the anchor.
        let anchor = Rect::new(100.0, 50.0, 200.0, 36.0);
        let ov = d.overlay_at(anchor, 800.0, 600.0, None, &style).unwrap();
        assert_eq!(ov.id.as_deref(), Some("dd-struct::backdrop"));
        let menu = &ov.children[0];
        use sabitori_core::element::Dimension;
        assert_eq!(menu.style.inset_left, Dimension::Px(100.0));
        assert_eq!(menu.style.inset_top, Dimension::Px(88.0));

        // Near the bottom edge → flips above.
        let anchor = Rect::new(100.0, 560.0, 200.0, 36.0);
        let ov = d.overlay_at(anchor, 800.0, 600.0, None, &style).unwrap();
        let menu = &ov.children[0];
        if let Dimension::Px(top) = menu.style.inset_top {
            assert!(top < 560.0, "menu must open upward, got {top}");
            assert!(top >= 0.0);
        } else {
            panic!("menu top must be Px");
        }
    }

    #[test]
    fn overlay_none_when_closed() {
        let style = DropdownStyle::default_dark();
        let d = dd();
        assert!(d
            .overlay_at(Rect::new(0.0, 0.0, 100.0, 36.0), 800.0, 600.0, None, &style)
            .is_none());
    }

    fn collect_ids(el: &Element) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(el: &Element, out: &mut Vec<String>) {
            if let Some(ref id) = el.id {
                out.push(id.clone());
            }
            for c in &el.children {
                walk(c, out);
            }
        }
        walk(el, &mut out);
        out
    }
}
