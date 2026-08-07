use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Point, Rect};

/// Style for dropdown.
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

/// Dropdown/select widget.
pub struct Dropdown {
    pub bounds: Rect,
    pub items: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub hovered_item: Option<usize>,
    pub style: DropdownStyle,
    /// Animation for menu open/close.
    pub open_anim: Animated<f32>,
}

impl Dropdown {
    pub fn new(bounds: Rect, items: Vec<String>, style: DropdownStyle) -> Self {
        Self {
            bounds,
            items,
            selected: 0,
            open: false,
            hovered_item: None,
            style,
            open_anim: Animated::new(0.0).with_spring(Spring::snappy()),
        }
    }

    /// The label currently shown.
    pub fn selected_label(&self) -> &str {
        self.items.get(self.selected).map(|s| s.as_str()).unwrap_or("")
    }

    /// The rect for the dropdown button (closed state).
    pub fn button_rect(&self) -> Rect {
        Rect::new(
            self.bounds.origin.x,
            self.bounds.origin.y,
            self.bounds.size.width,
            self.style.height,
        )
    }

    /// The rect for the dropdown menu (open state).
    pub fn menu_rect(&self) -> Rect {
        let visible = self.items.len().min(self.style.max_visible_items);
        let menu_height = visible as f32 * self.style.item_height;
        Rect::new(
            self.bounds.origin.x,
            self.bounds.origin.y + self.style.height,
            self.bounds.size.width,
            menu_height * self.open_anim.value(),
        )
    }

    /// The rect for a menu item.
    pub fn item_rect(&self, idx: usize) -> Rect {
        Rect::new(
            self.bounds.origin.x,
            self.bounds.origin.y + self.style.height + idx as f32 * self.style.item_height,
            self.bounds.size.width,
            self.style.item_height,
        )
    }

    /// Toggle open/close.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.open_anim.set_target(if self.open { 1.0 } else { 0.0 });
        self.hovered_item = None;
    }

    /// Close the menu.
    pub fn close(&mut self) {
        self.open = false;
        self.open_anim.set_target(0.0);
        self.hovered_item = None;
    }

    /// Handle pointer move.
    pub fn on_pointer_move(&mut self, point: Point) {
        if !self.open {
            return;
        }
        let menu = self.menu_rect();
        if !menu.contains(point) {
            self.hovered_item = None;
            return;
        }
        let local_y = point.y - menu.origin.y;
        let idx = (local_y / self.style.item_height) as usize;
        if idx < self.items.len() {
            self.hovered_item = Some(idx);
        } else {
            self.hovered_item = None;
        }
    }

    /// Handle click. Returns Some(idx) if an item was selected.
    pub fn on_click(&mut self, point: Point) -> Option<usize> {
        // Click on button → toggle
        if self.button_rect().contains(point) {
            self.toggle();
            return None;
        }

        // Click on menu item → select
        if self.open {
            let menu = self.menu_rect();
            if menu.contains(point) {
                let local_y = point.y - menu.origin.y;
                let idx = (local_y / self.style.item_height) as usize;
                if idx < self.items.len() {
                    self.selected = idx;
                    self.close();
                    return Some(idx);
                }
            }
            // Click outside → close
            self.close();
        }

        None
    }

    /// Select next item (keyboard).
    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.open {
            self.hovered_item = Some(match self.hovered_item {
                Some(i) if i + 1 < self.items.len() => i + 1,
                _ => 0,
            });
        } else {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    /// Select previous item (keyboard).
    pub fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.open {
            self.hovered_item = Some(match self.hovered_item {
                Some(0) | None => self.items.len() - 1,
                Some(i) => i - 1,
            });
        } else {
            self.selected = if self.selected == 0 { self.items.len() - 1 } else { self.selected - 1 };
        }
    }

    /// Confirm selection (Enter key when open).
    pub fn confirm(&mut self) -> Option<usize> {
        if self.open {
            if let Some(idx) = self.hovered_item {
                self.selected = idx;
                self.close();
                return Some(idx);
            }
            self.close();
        } else {
            self.toggle();
        }
        None
    }

    /// Get background color for a menu item.
    pub fn item_bg(&self, idx: usize) -> Color {
        if self.hovered_item == Some(idx) {
            self.style.menu_item_hover
        } else {
            self.style.menu_bg
        }
    }

    /// Get foreground color for a menu item.
    pub fn item_fg(&self, idx: usize) -> Color {
        if idx == self.selected {
            self.style.fg_selected
        } else {
            self.style.fg
        }
    }

    /// Tick animations.
    pub fn tick(&mut self, dt: f32) {
        self.open_anim.tick(dt);
    }
}
