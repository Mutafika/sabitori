use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Element};
use sabitori_core::element::{div, text, Px, JustifyContent};

/// Definition for a single item in a context menu or menu bar dropdown.
#[derive(Clone)]
pub struct MenuItemDef {
    pub id: String,
    pub label: String,
    pub shortcut: String,
    pub enabled: bool,
    pub separator: bool,
    /// Child items for a one-level submenu (flyout). Empty for plain
    /// action items. Rendered by `MenuBarState`; `ContextMenuState`
    /// currently ignores it.
    pub submenu: Vec<MenuItemDef>,
}

impl MenuItemDef {
    /// Create a standard action menu item.
    pub fn action(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shortcut: String::new(),
            enabled: true,
            separator: false,
            submenu: Vec::new(),
        }
    }

    /// Set a keyboard shortcut label (e.g. "Cmd+C").
    pub fn with_shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = s.into();
        self
    }

    /// Mark this item as disabled (greyed out, not clickable).
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Attach a one-level submenu (flyout) to this item. The item itself
    /// stops being an action: clicking / hovering it opens the submenu.
    pub fn with_submenu(mut self, items: Vec<MenuItemDef>) -> Self {
        self.submenu = items;
        self
    }

    /// Whether this item opens a submenu.
    pub fn has_submenu(&self) -> bool {
        !self.submenu.is_empty()
    }

    /// Create a separator line (not clickable).
    pub fn separator() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            shortcut: String::new(),
            enabled: false,
            separator: true,
            submenu: Vec::new(),
        }
    }
}

/// State for a context menu overlay.
pub struct ContextMenuState {
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    items: Vec<MenuItemDef>,
    pub opacity: Animated<f32>,
}

impl ContextMenuState {
    pub fn new() -> Self {
        Self {
            visible: false,
            x: 0.0,
            y: 0.0,
            items: Vec::new(),
            opacity: Animated::new(0.0).with_spring(Spring::snappy()),
        }
    }

    /// Show the context menu at the given position with the given items.
    pub fn show(&mut self, x: f32, y: f32, items: Vec<MenuItemDef>) {
        self.visible = true;
        self.x = x;
        self.y = y;
        self.items = items;
        self.opacity.set_target(1.0);
    }

    /// Dismiss the context menu with a fade-out animation.
    pub fn dismiss(&mut self) {
        self.opacity.set_target(0.0);
        self.visible = false;
    }

    /// Whether the menu is currently visible (or animating out).
    pub fn is_visible(&self) -> bool {
        self.visible || self.opacity.value() > 0.01
    }

    /// Advance animations. Call each frame with delta time in seconds.
    pub fn tick(&mut self, dt: f32) {
        self.opacity.tick(dt);
    }

    /// Get a reference to the current menu items.
    pub fn items(&self) -> &[MenuItemDef] {
        &self.items
    }

    /// Build the overlay Element for this menu. Returns `None` if not visible.
    ///
    /// * `viewport_w`, `viewport_h` — viewport dimensions for backdrop sizing and clamping.
    /// * `hovered` — ID of the currently hovered item, for highlighting.
    /// * `bg` — menu background color.
    /// * `border` — menu border color.
    /// * `text_color` — primary text color for item labels.
    /// * `text_sec` — secondary text color for shortcuts.
    /// * `hover_bg` — background color for the hovered item.
    pub fn to_overlay(
        &self,
        viewport_w: f32,
        viewport_h: f32,
        hovered: Option<&str>,
        bg: Color,
        border: Color,
        text_color: Color,
        text_sec: Color,
        hover_bg: Color,
    ) -> Option<Element> {
        if !self.is_visible() {
            return None;
        }

        let opacity = self.opacity.value();

        // Menu dimensions
        let menu_w: f32 = 220.0;
        let item_h: f32 = 30.0;
        let separator_h: f32 = 1.0;
        let menu_padding: f32 = 4.0;
        let menu_h: f32 = self.items.iter().fold(menu_padding * 2.0, |acc, item| {
            acc + if item.separator { separator_h + 4.0 } else { item_h }
        });

        // Clamp position so the menu stays within the viewport
        let clamped_x = self.x.min(viewport_w - menu_w - 4.0).max(0.0);
        let clamped_y = self.y.min(viewport_h - menu_h - 4.0).max(0.0);

        // Build menu items
        let mut menu_children: Vec<Element> = Vec::new();
        for item in &self.items {
            if item.separator {
                // Separator line
                menu_children.push(
                    div()
                        .w_full()
                        .h(Px(separator_h))
                        .bg(border)
                        .my(Px(2.0))
                );
            } else {
                let is_hovered = hovered == Some(item.id.as_str()) && item.enabled;
                let item_bg = if is_hovered { hover_bg } else { Color::TRANSPARENT };
                let label_color = if item.enabled { text_color } else { text_sec };

                let mut row = div()
                    .id(&item.id)
                    .w_full()
                    .h(Px(item_h))
                    .bg(item_bg)
                    .rounded_px(4.0)
                    .px_pad(Px(12.0))
                    .flex_row()
                    .items_center()
                    .justify_content(JustifyContent::SpaceBetween);

                let label_el = text(&item.label)
                    .font_size(13.0)
                    .color(label_color);

                if item.shortcut.is_empty() {
                    row = row.child(label_el);
                } else {
                    row = row
                        .child(label_el)
                        .child(
                            text(&item.shortcut)
                                .font_size(11.0)
                                .color(text_sec)
                        );
                }

                menu_children.push(row);
            }
        }

        // Positioned menu panel
        let menu_panel = div()
            .pos(clamped_x, clamped_y)
            .w(Px(menu_w))
            .h(Px(menu_h))
            .bg(bg)
            .border(1.0, border)
            .rounded_px(8.0)
            .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
            .opacity(opacity)
            .p(Px(menu_padding))
            .flex_col()
            .overflow_hidden()
            .children(menu_children);

        // Full viewport backdrop + menu
        let overlay = div()
            .id("ctx-backdrop")
            .w(Px(viewport_w))
            .h(Px(viewport_h))
            .pos(0.0, 0.0)
            .overlay()
            .child(menu_panel);

        Some(overlay)
    }
}

impl Default for ContextMenuState {
    fn default() -> Self {
        Self::new()
    }
}
