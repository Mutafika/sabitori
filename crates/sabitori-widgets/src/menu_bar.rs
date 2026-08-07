//! Horizontal menu bar (File / Edit / View …) with dropdown menus,
//! hover-to-switch between neighboring menus, one-level submenus,
//! keyboard-shortcut labels, and separators.
//!
//! [`ContextMenuState`](crate::ContextMenuState) を土台に、 アプリ上端の
//! 常設メニューバーへ一般化したもの。 Reuses [`MenuItemDef`] for items.
//!
//! ## Architecture
//!
//! State and visuals are split, sabitori 流:
//!
//! * [`MenuBarState`] owns which menu / submenu is open and interprets
//!   clicks + hover changes by element id (no rect math in the app).
//! * [`MenuBarState::bar`] builds the always-visible bar for `view()`.
//! * [`MenuBarState::overlay`] builds the open dropdown for
//!   `overlay_view()`. It re-renders an **invisible replica** of the bar
//!   inside the overlay so the dropdown lines up with its label without
//!   the widget ever knowing layout rects: both trees use identical
//!   labels / fonts / paddings, so Taffy gives them identical x
//!   positions. The replica labels carry the same ids as the real bar,
//!   which is also what lets hover-to-switch work *through* the
//!   backdrop.
//!
//! ## Wiring (DeclarativeApp)
//!
//! ```ignore
//! fn view(&self, ctx) -> Element {
//!     div().flex_col().children([
//!         self.menu_bar.bar(&self.menus, ctx.hovered.as_deref(), &self.menu_style),
//!         /* rest of the app */
//!     ])
//! }
//! fn overlay_view(&self, ctx) -> Option<Element> {
//!     self.menu_bar.overlay(&self.menus, ctx.width, ctx.height,
//!                           ctx.hovered.as_deref(), &self.menu_style)
//! }
//! fn on_click(&mut self, id: &str) {
//!     if let Some(action) = self.menu_bar.handle_click(id, &self.menus) {
//!         self.run_menu_action(&action);
//!     }
//! }
//! fn on_hover_change(&mut self, id: Option<&str>) {
//!     self.menu_bar.handle_hover(id, &self.menus);
//! }
//! ```

use sabitori_core::element::{div, text, JustifyContent, Px};
use sabitori_core::{Color, Element};

use crate::context_menu_widget::MenuItemDef;

/// One top-level menu in the bar ("File", "Edit", …).
#[derive(Clone)]
pub struct MenuDef {
    /// Stable id for the menu (used in element ids).
    pub id: String,
    /// Label shown in the bar.
    pub label: String,
    /// Dropdown items.
    pub items: Vec<MenuItemDef>,
}

impl MenuDef {
    pub fn new(id: impl Into<String>, label: impl Into<String>, items: Vec<MenuItemDef>) -> Self {
        Self { id: id.into(), label: label.into(), items }
    }
}

/// Visual parameters for the menu bar. Same pattern as
/// [`DropdownStyle`](crate::DropdownStyle).
#[derive(Clone, Debug)]
pub struct MenuBarStyle {
    pub bar_bg: Color,
    pub label_fg: Color,
    pub label_hover_bg: Color,
    /// Background of the open menu's bar label.
    pub label_open_bg: Color,
    pub menu_bg: Color,
    pub menu_border: Color,
    pub item_fg: Color,
    pub item_disabled_fg: Color,
    pub shortcut_fg: Color,
    pub item_hover_bg: Color,
    pub bar_height: f32,
    pub item_height: f32,
    pub menu_width: f32,
    pub font_size: f32,
    /// Horizontal padding inside each bar label.
    pub label_padding_x: f32,
}

impl MenuBarStyle {
    pub fn default_dark() -> Self {
        Self {
            bar_bg: Color::from_hex("#1a1a2e"),
            label_fg: Color::from_hex("#c8c8dc"),
            label_hover_bg: Color::from_hex("#2a2a48"),
            label_open_bg: Color::from_hex("#34345a"),
            menu_bg: Color::from_hex("#1e1e32"),
            menu_border: Color::from_hex("#3a3a55"),
            item_fg: Color::from_hex("#d8d8ea"),
            item_disabled_fg: Color::from_hex("#5a5a72"),
            shortcut_fg: Color::from_hex("#8a8aa4"),
            item_hover_bg: Color::from_hex("#2e2e52"),
            bar_height: 28.0,
            item_height: 26.0,
            menu_width: 230.0,
            font_size: 13.0,
            label_padding_x: 12.0,
        }
    }
}

/// Backdrop id used by the overlay; clicking it closes the menu.
const BACKDROP_ID: &str = "__menubar-backdrop";
const LABEL_PREFIX: &str = "__menubar:";
const ITEM_PREFIX: &str = "__menuitem:";

/// Element id for a top-level menu label.
fn label_id(menu: &MenuDef) -> String {
    format!("{LABEL_PREFIX}{}", menu.id)
}

/// Element id for a dropdown / submenu item.
fn item_id(item: &MenuItemDef) -> String {
    format!("{ITEM_PREFIX}{}", item.id)
}

/// State for a menu bar: which menu and submenu are open.
pub struct MenuBarState {
    /// Index of the open top-level menu, if any.
    open: Option<usize>,
    /// Index (within the open menu's items) of the item whose submenu
    /// is open, if any.
    submenu_open: Option<usize>,
}

impl MenuBarState {
    pub fn new() -> Self {
        Self { open: None, submenu_open: None }
    }

    /// Whether any dropdown is currently open.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Index of the open top-level menu.
    pub fn open_index(&self) -> Option<usize> {
        self.open
    }

    /// Close everything.
    pub fn close(&mut self) {
        self.open = None;
        self.submenu_open = None;
    }

    /// Interpret a click on element `clicked_id` (from
    /// `DeclarativeApp::on_click`). Returns `Some(action_id)` when a leaf
    /// menu item was activated — the menu closes itself. Returns `None`
    /// for structural clicks (open/close/switch/submenu/backdrop) and
    /// for ids unrelated to the menu bar.
    pub fn handle_click(&mut self, clicked_id: &str, menus: &[MenuDef]) -> Option<String> {
        // Top-level label → toggle.
        if let Some(rest) = clicked_id.strip_prefix(LABEL_PREFIX) {
            let idx = menus.iter().position(|m| m.id == rest)?;
            if self.open == Some(idx) {
                self.close();
            } else {
                self.open = Some(idx);
                self.submenu_open = None;
            }
            return None;
        }
        // Backdrop → close.
        if clicked_id == BACKDROP_ID {
            self.close();
            return None;
        }
        // Dropdown / submenu item.
        if let Some(rest) = clicked_id.strip_prefix(ITEM_PREFIX) {
            let menu = &menus[self.open?];
            // Top-level dropdown items first.
            if let Some((idx, item)) = menu
                .items
                .iter()
                .enumerate()
                .find(|(_, it)| !it.separator && it.id == rest)
            {
                if !item.enabled {
                    return None; // disabled: ignore, stay open
                }
                if item.has_submenu() {
                    // Click on a submenu parent toggles its flyout.
                    self.submenu_open =
                        if self.submenu_open == Some(idx) { None } else { Some(idx) };
                    return None;
                }
                self.close();
                return Some(item.id.clone());
            }
            // Then items inside the open submenu.
            if let Some(parent_idx) = self.submenu_open {
                if let Some(item) = menu.items[parent_idx]
                    .submenu
                    .iter()
                    .find(|it| !it.separator && it.id == rest)
                {
                    if !item.enabled {
                        return None;
                    }
                    self.close();
                    return Some(item.id.clone());
                }
            }
            return None;
        }
        None
    }

    /// Interpret a hover change (from `DeclarativeApp::on_hover_change`).
    /// While open: hovering a *different* top-level label switches the
    /// open menu (the native menu-bar gesture); hovering an item with a
    /// submenu opens its flyout; hovering a plain sibling item closes
    /// the flyout. No-op when closed.
    pub fn handle_hover(&mut self, hovered_id: Option<&str>, menus: &[MenuDef]) {
        let Some(open_idx) = self.open else { return };
        let Some(id) = hovered_id else { return };

        if let Some(rest) = id.strip_prefix(LABEL_PREFIX) {
            if let Some(idx) = menus.iter().position(|m| m.id == rest) {
                if idx != open_idx {
                    self.open = Some(idx);
                    self.submenu_open = None;
                }
            }
            return;
        }
        if let Some(rest) = id.strip_prefix(ITEM_PREFIX) {
            let menu = &menus[open_idx];
            if let Some((idx, item)) = menu
                .items
                .iter()
                .enumerate()
                .find(|(_, it)| !it.separator && it.id == rest)
            {
                self.submenu_open = if item.has_submenu() && item.enabled {
                    Some(idx)
                } else {
                    None
                };
            }
            // Hovering a submenu's own items keeps the flyout open
            // (they're not found among the top-level items → no change).
        }
    }

    // ── Element builders ──────────────────────────────────────────

    /// Build the always-visible bar for `view()`.
    pub fn bar(&self, menus: &[MenuDef], hovered: Option<&str>, style: &MenuBarStyle) -> Element {
        let labels: Vec<Element> = menus
            .iter()
            .enumerate()
            .map(|(idx, menu)| self.label_cell(menu, idx, hovered, style, true))
            .collect();
        div()
            .w_full()
            .h(Px(style.bar_height))
            .bg(style.bar_bg)
            .flex_row()
            .items_center()
            .children(labels)
    }

    /// Build the dropdown overlay for `overlay_view()`. `None` when closed.
    ///
    /// The overlay is a full-viewport backdrop (catches outside clicks)
    /// containing an invisible replica of the bar; the open label's cell
    /// carries the dropdown as an absolutely-positioned child, so it
    /// hangs exactly below its label. Note: a dropdown anchored near the
    /// right edge can extend past the viewport (no clamping in v1 — the
    /// anchor x is never known numerically).
    pub fn overlay(
        &self,
        menus: &[MenuDef],
        viewport_w: f32,
        viewport_h: f32,
        hovered: Option<&str>,
        style: &MenuBarStyle,
    ) -> Option<Element> {
        let open_idx = self.open?;
        if open_idx >= menus.len() {
            return None;
        }

        let labels: Vec<Element> = menus
            .iter()
            .enumerate()
            .map(|(idx, menu)| {
                let mut cell = self.label_cell(menu, idx, hovered, style, false);
                if idx == open_idx {
                    cell = cell.child(
                        self.dropdown_panel(&menus[open_idx], hovered, style)
                            .pos(0.0, style.bar_height),
                    );
                }
                cell
            })
            .collect();

        let replica_bar = div()
            .w_full()
            .h(Px(style.bar_height))
            .flex_row()
            .items_center()
            .children(labels);

        Some(
            div()
                .id(BACKDROP_ID)
                .w(Px(viewport_w))
                .h(Px(viewport_h))
                .pos(0.0, 0.0)
                .overlay()
                .child(replica_bar),
        )
    }

    /// One bar label cell. `visible: false` builds the overlay replica:
    /// identical geometry (same text + font + padding → same Taffy
    /// layout) but fully transparent, so only the real bar below shows.
    fn label_cell(
        &self,
        menu: &MenuDef,
        idx: usize,
        hovered: Option<&str>,
        style: &MenuBarStyle,
        visible: bool,
    ) -> Element {
        let id = label_id(menu);
        let bg = if !visible {
            Color::TRANSPARENT
        } else if self.open == Some(idx) {
            style.label_open_bg
        } else if hovered == Some(id.as_str()) {
            style.label_hover_bg
        } else {
            Color::TRANSPARENT
        };
        let fg = if visible { style.label_fg } else { Color::TRANSPARENT };
        div()
            .id(&id)
            .h(Px(style.bar_height))
            .bg(bg)
            .px_pad(Px(style.label_padding_x))
            .flex_row()
            .items_center()
            .shrink(0.0)
            .child(
                text(&menu.label)
                    .font_size(style.font_size)
                    .color(fg)
                    .shrink(0.0),
            )
    }

    /// The open dropdown panel (and its submenu flyout, if open).
    fn dropdown_panel(
        &self,
        menu: &MenuDef,
        hovered: Option<&str>,
        style: &MenuBarStyle,
    ) -> Element {
        let menu_padding: f32 = 4.0;
        let separator_h: f32 = 1.0;

        let mut children: Vec<Element> = Vec::new();
        // Y offset of each row within the panel, for submenu anchoring.
        let mut y = menu_padding;
        let mut submenu_panel: Option<Element> = None;

        for (idx, item) in menu.items.iter().enumerate() {
            if item.separator {
                children.push(
                    div().w_full().h(Px(separator_h)).bg(style.menu_border).my(Px(2.0)),
                );
                y += separator_h + 4.0;
                continue;
            }
            children.push(self.item_row(item, hovered, style, item.has_submenu()));
            if self.submenu_open == Some(idx) && item.has_submenu() {
                submenu_panel = Some(
                    self.submenu_flyout(item, hovered, style)
                        .pos(style.menu_width - 6.0, y),
                );
            }
            y += style.item_height;
        }

        let mut panel = div()
            .w(Px(style.menu_width))
            .bg(style.menu_bg)
            .border(1.0, style.menu_border)
            .rounded_px(8.0)
            .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
            .p(Px(menu_padding))
            .flex_col()
            .children(children);
        if let Some(sub) = submenu_panel {
            panel = panel.child(sub);
        }
        panel
    }

    /// The submenu flyout panel for `parent`.
    fn submenu_flyout(
        &self,
        parent: &MenuItemDef,
        hovered: Option<&str>,
        style: &MenuBarStyle,
    ) -> Element {
        let rows: Vec<Element> = parent
            .submenu
            .iter()
            .map(|item| {
                if item.separator {
                    div().w_full().h(Px(1.0)).bg(style.menu_border).my(Px(2.0))
                } else {
                    self.item_row(item, hovered, style, false)
                }
            })
            .collect();
        div()
            .w(Px(style.menu_width))
            .bg(style.menu_bg)
            .border(1.0, style.menu_border)
            .rounded_px(8.0)
            .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
            .p(Px(4.0))
            .flex_col()
            .children(rows)
    }

    /// One dropdown row: label left, shortcut or `▶` right.
    fn item_row(
        &self,
        item: &MenuItemDef,
        hovered: Option<&str>,
        style: &MenuBarStyle,
        submenu_arrow: bool,
    ) -> Element {
        let id = item_id(item);
        let is_hovered = item.enabled && hovered == Some(id.as_str());
        let bg = if is_hovered { style.item_hover_bg } else { Color::TRANSPARENT };
        let fg = if item.enabled { style.item_fg } else { style.item_disabled_fg };

        let mut row = div()
            .id(&id)
            .w_full()
            .h(Px(style.item_height))
            .bg(bg)
            .rounded_px(4.0)
            .px_pad(Px(10.0))
            .flex_row()
            .items_center()
            .justify_content(JustifyContent::SpaceBetween)
            .child(text(&item.label).font_size(style.font_size).color(fg));

        if submenu_arrow {
            row = row.child(text("\u{25B6}").font_size(9.0).color(style.shortcut_fg));
        } else if !item.shortcut.is_empty() {
            row = row.child(
                text(&item.shortcut)
                    .font_size(11.0)
                    .color(style.shortcut_fg),
            );
        }
        row
    }
}

impl Default for MenuBarState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menus() -> Vec<MenuDef> {
        vec![
            MenuDef::new(
                "file",
                "File",
                vec![
                    MenuItemDef::action("new", "New").with_shortcut("Cmd+N"),
                    MenuItemDef::action("open", "Open…"),
                    MenuItemDef::separator(),
                    MenuItemDef::action("export", "Export").with_submenu(vec![
                        MenuItemDef::action("export-ifc", "IFC…"),
                        MenuItemDef::action("export-dxf", "DXF…"),
                    ]),
                    MenuItemDef::action("locked", "Locked").disabled(),
                ],
            ),
            MenuDef::new("edit", "Edit", vec![MenuItemDef::action("undo", "Undo")]),
        ]
    }

    #[test]
    fn click_label_opens_and_toggles() {
        let m = menus();
        let mut s = MenuBarState::new();
        assert!(!s.is_open());
        assert_eq!(s.handle_click("__menubar:file", &m), None);
        assert_eq!(s.open_index(), Some(0));
        // Click again → closes.
        assert_eq!(s.handle_click("__menubar:file", &m), None);
        assert!(!s.is_open());
    }

    #[test]
    fn click_other_label_switches() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        s.handle_click("__menubar:edit", &m);
        assert_eq!(s.open_index(), Some(1));
    }

    #[test]
    fn action_click_closes_and_returns_id() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        let action = s.handle_click("__menuitem:new", &m);
        assert_eq!(action.as_deref(), Some("new"));
        assert!(!s.is_open());
    }

    #[test]
    fn disabled_item_is_ignored_and_menu_stays_open() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        assert_eq!(s.handle_click("__menuitem:locked", &m), None);
        assert!(s.is_open());
    }

    #[test]
    fn backdrop_click_closes() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        assert_eq!(s.handle_click("__menubar-backdrop", &m), None);
        assert!(!s.is_open());
    }

    #[test]
    fn hover_switches_open_menu() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        s.handle_hover(Some("__menubar:edit"), &m);
        assert_eq!(s.open_index(), Some(1));
        assert_eq!(s.submenu_open, None);
    }

    #[test]
    fn hover_does_nothing_when_closed() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_hover(Some("__menubar:edit"), &m);
        assert!(!s.is_open());
    }

    #[test]
    fn hover_submenu_parent_opens_flyout_and_sibling_closes_it() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        s.handle_hover(Some("__menuitem:export"), &m);
        assert_eq!(s.submenu_open, Some(3));
        // Hovering a plain sibling closes the flyout.
        s.handle_hover(Some("__menuitem:open"), &m);
        assert_eq!(s.submenu_open, None);
    }

    #[test]
    fn submenu_item_click_closes_and_returns_id() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        s.handle_hover(Some("__menuitem:export"), &m);
        let action = s.handle_click("__menuitem:export-ifc", &m);
        assert_eq!(action.as_deref(), Some("export-ifc"));
        assert!(!s.is_open());
    }

    #[test]
    fn submenu_parent_click_toggles_flyout_without_action() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        assert_eq!(s.handle_click("__menuitem:export", &m), None);
        assert_eq!(s.submenu_open, Some(3));
        assert_eq!(s.handle_click("__menuitem:export", &m), None);
        assert_eq!(s.submenu_open, None);
        assert!(s.is_open(), "toggling a flyout must not close the menu");
    }

    #[test]
    fn unrelated_ids_are_ignored() {
        let m = menus();
        let mut s = MenuBarState::new();
        s.handle_click("__menubar:file", &m);
        assert_eq!(s.handle_click("some-button", &m), None);
        assert!(s.is_open());
        s.handle_hover(Some("some-button"), &m);
        assert_eq!(s.open_index(), Some(0));
    }

    #[test]
    fn overlay_only_when_open() {
        let m = menus();
        let style = MenuBarStyle::default_dark();
        let mut s = MenuBarState::new();
        assert!(s.overlay(&m, 800.0, 600.0, None, &style).is_none());
        s.handle_click("__menubar:file", &m);
        let overlay = s.overlay(&m, 800.0, 600.0, None, &style).expect("open");
        assert_eq!(overlay.id.as_deref(), Some("__menubar-backdrop"));
    }

    #[test]
    fn bar_labels_carry_stable_ids() {
        let m = menus();
        let style = MenuBarStyle::default_dark();
        let s = MenuBarState::new();
        let bar = s.bar(&m, None, &style);
        let ids: Vec<_> = bar.children.iter().filter_map(|c| c.id.clone()).collect();
        assert_eq!(ids, vec!["__menubar:file", "__menubar:edit"]);
    }
}
