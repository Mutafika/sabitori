use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Point, Rect};

/// Column definition for a table.
#[derive(Clone, Debug)]
pub struct TableColumn {
    /// Column header label.
    pub label: String,
    /// Width fraction (0.0-1.0). All fractions are normalized.
    pub width: f32,
}

impl TableColumn {
    pub fn new(label: &str, width: f32) -> Self {
        Self {
            label: label.to_string(),
            width,
        }
    }
}

/// A single cell value.
#[derive(Clone, Debug)]
pub struct CellValue {
    pub text: String,
    pub color: Option<Color>,
    pub bold: bool,
}

impl CellValue {
    pub fn text(s: &str) -> Self {
        Self {
            text: s.to_string(),
            color: None,
            bold: false,
        }
    }

    pub fn colored(s: &str, color: Color) -> Self {
        Self {
            text: s.to_string(),
            color: Some(color),
            bold: false,
        }
    }

    pub fn bold(s: &str) -> Self {
        Self {
            text: s.to_string(),
            color: None,
            bold: true,
        }
    }
}

/// Style configuration for the table.
#[derive(Clone, Debug)]
pub struct TableStyle {
    pub header_bg: Color,
    pub header_fg: Color,
    pub row_bg: Color,
    pub row_alt_bg: Color,
    pub row_hover_bg: Color,
    pub row_selected_bg: Color,
    pub row_fg: Color,
    pub row_selected_fg: Color,
    pub border_color: Color,
    pub row_height: f32,
    pub header_height: f32,
    pub padding_x: f32,
    pub corner_radius: f32,
}

impl TableStyle {
    pub fn default_dark() -> Self {
        Self {
            header_bg: Color::from_hex("#1a1a2e"),
            header_fg: Color::from_hex("#8888aa"),
            row_bg: Color::from_hex("#16161e"),
            row_alt_bg: Color::from_hex("#1a1a28"),
            row_hover_bg: Color::from_hex("#24243a"),
            row_selected_bg: Color::from_hex("#2a2a50"),
            row_fg: Color::from_hex("#c8c8dc"),
            row_selected_fg: Color::from_hex("#ffffff"),
            border_color: Color::from_hex("#2a2a40"),
            row_height: 32.0,
            header_height: 36.0,
            padding_x: 12.0,
            corner_radius: 8.0,
        }
    }
}

/// Table widget state.
pub struct Table {
    pub bounds: Rect,
    pub columns: Vec<TableColumn>,
    pub rows: Vec<Vec<CellValue>>,
    pub style: TableStyle,

    /// Currently selected row index.
    pub selected: Option<usize>,
    /// Currently hovered row index.
    pub hovered: Option<usize>,
    /// Scroll offset for virtual scrolling.
    pub scroll_y: Animated<f32>,
    /// Sort column index and ascending flag.
    pub sort: Option<(usize, bool)>,

    // Animation
    pub hover_anim: Animated<f32>,
    prev_hovered: Option<usize>,
}

impl Table {
    pub fn new(bounds: Rect, columns: Vec<TableColumn>, style: TableStyle) -> Self {
        Self {
            bounds,
            columns,
            rows: Vec::new(),
            style,
            selected: None,
            hovered: None,
            scroll_y: Animated::new(0.0).with_spring(Spring::critical(150.0)),
            sort: None,
            hover_anim: Animated::new(0.0).with_spring(Spring::snappy()),
            prev_hovered: None,
        }
    }

    /// Set the data rows.
    pub fn set_rows(&mut self, rows: Vec<Vec<CellValue>>) {
        self.rows = rows;
    }

    /// Total content height.
    pub fn content_height(&self) -> f32 {
        self.style.header_height + self.rows.len() as f32 * self.style.row_height
    }

    /// Viewport height (excluding header).
    pub fn viewport_height(&self) -> f32 {
        self.bounds.size.height - self.style.header_height
    }

    /// Max scroll offset.
    pub fn max_scroll(&self) -> f32 {
        (self.rows.len() as f32 * self.style.row_height - self.viewport_height()).max(0.0)
    }

    /// Handle scroll input.
    pub fn on_scroll(&mut self, delta_y: f32) {
        let current = self.scroll_y.value();
        let new_target = (current - delta_y).clamp(0.0, self.max_scroll());
        self.scroll_y.set_target(new_target);
    }

    /// Get the range of visible rows (start_index, end_index).
    pub fn visible_range(&self) -> (usize, usize) {
        let scroll = self.scroll_y.value();
        let first = (scroll / self.style.row_height).floor() as usize;
        let count = (self.viewport_height() / self.style.row_height).ceil() as usize + 2;
        let last = (first + count).min(self.rows.len());
        (first, last)
    }

    /// Handle pointer move — update hover state.
    pub fn on_pointer_move(&mut self, point: Point) {
        if !self.bounds.contains(point) {
            self.hovered = None;
            return;
        }

        let local_y = point.y - self.bounds.origin.y - self.style.header_height + self.scroll_y.value();
        if local_y < 0.0 {
            self.hovered = None;
            return;
        }

        let row_idx = (local_y / self.style.row_height) as usize;
        if row_idx < self.rows.len() {
            self.hovered = Some(row_idx);
        } else {
            self.hovered = None;
        }
    }

    /// Handle click — select row or sort column.
    pub fn on_click(&mut self, point: Point) -> TableEvent {
        if !self.bounds.contains(point) {
            return TableEvent::None;
        }

        let local_y = point.y - self.bounds.origin.y;

        // Click on header → sort
        if local_y < self.style.header_height {
            let col_idx = self.column_at_x(point.x);
            if let Some(idx) = col_idx {
                self.sort = Some(match self.sort {
                    Some((prev_col, asc)) if prev_col == idx => (idx, !asc),
                    _ => (idx, true),
                });
                return TableEvent::Sort(idx, self.sort.unwrap().1);
            }
            return TableEvent::None;
        }

        // Click on row → select
        let scroll = self.scroll_y.value();
        let row_y = local_y - self.style.header_height + scroll;
        let row_idx = (row_y / self.style.row_height) as usize;
        if row_idx < self.rows.len() {
            self.selected = Some(row_idx);
            return TableEvent::Select(row_idx);
        }

        TableEvent::None
    }

    /// Handle double-click on a row.
    pub fn on_double_click(&mut self, point: Point) -> TableEvent {
        if !self.bounds.contains(point) {
            return TableEvent::None;
        }

        let local_y = point.y - self.bounds.origin.y - self.style.header_height + self.scroll_y.value();
        if local_y < 0.0 {
            return TableEvent::None;
        }

        let row_idx = (local_y / self.style.row_height) as usize;
        if row_idx < self.rows.len() {
            self.selected = Some(row_idx);
            return TableEvent::Activate(row_idx);
        }

        TableEvent::None
    }

    /// Select next/previous row.
    pub fn select_next(&mut self) {
        let len = self.rows.len();
        if len == 0 { return; }
        self.selected = Some(match self.selected {
            Some(i) if i + 1 < len => i + 1,
            Some(_) => 0,
            None => 0,
        });
        self.ensure_visible();
    }

    pub fn select_prev(&mut self) {
        let len = self.rows.len();
        if len == 0 { return; }
        self.selected = Some(match self.selected {
            Some(0) => len - 1,
            Some(i) => i - 1,
            None => 0,
        });
        self.ensure_visible();
    }

    /// Ensure selected row is visible (scroll if needed).
    fn ensure_visible(&mut self) {
        if let Some(idx) = self.selected {
            let row_top = idx as f32 * self.style.row_height;
            let row_bottom = row_top + self.style.row_height;
            let scroll = self.scroll_y.value();
            let viewport = self.viewport_height();

            if row_top < scroll {
                self.scroll_y.set_target(row_top);
            } else if row_bottom > scroll + viewport {
                self.scroll_y.set_target(row_bottom - viewport);
            }
        }
    }

    /// Get column widths in pixels based on table bounds.
    pub fn column_widths(&self) -> Vec<f32> {
        let total: f32 = self.columns.iter().map(|c| c.width).sum();
        let available = self.bounds.size.width;
        self.columns
            .iter()
            .map(|c| (c.width / total) * available)
            .collect()
    }

    /// Get x positions for each column.
    pub fn column_xs(&self) -> Vec<f32> {
        let widths = self.column_widths();
        let mut xs = Vec::with_capacity(widths.len());
        let mut x = self.bounds.origin.x;
        for w in &widths {
            xs.push(x);
            x += w;
        }
        xs
    }

    /// Find which column a given x coordinate is in.
    fn column_at_x(&self, x: f32) -> Option<usize> {
        let xs = self.column_xs();
        let widths = self.column_widths();
        for (i, (col_x, w)) in xs.iter().zip(widths.iter()).enumerate() {
            if x >= *col_x && x < col_x + w {
                return Some(i);
            }
        }
        None
    }

    /// Get the rect for a header cell.
    pub fn header_cell_rect(&self, col_idx: usize) -> Rect {
        let xs = self.column_xs();
        let widths = self.column_widths();
        Rect::new(
            xs[col_idx],
            self.bounds.origin.y,
            widths[col_idx],
            self.style.header_height,
        )
    }

    /// Get the rect for a data row.
    pub fn row_rect(&self, row_idx: usize) -> Rect {
        let scroll = self.scroll_y.value();
        let y = self.bounds.origin.y + self.style.header_height
            + row_idx as f32 * self.style.row_height
            - scroll;
        Rect::new(self.bounds.origin.x, y, self.bounds.size.width, self.style.row_height)
    }

    /// Get the rect for a specific cell.
    pub fn cell_rect(&self, row_idx: usize, col_idx: usize) -> Rect {
        let xs = self.column_xs();
        let widths = self.column_widths();
        let scroll = self.scroll_y.value();
        let y = self.bounds.origin.y + self.style.header_height
            + row_idx as f32 * self.style.row_height
            - scroll;
        Rect::new(xs[col_idx], y, widths[col_idx], self.style.row_height)
    }

    /// Tick animations.
    pub fn tick(&mut self, dt: f32) {
        self.scroll_y.tick(dt);
        self.hover_anim.tick(dt);

        if self.prev_hovered != self.hovered {
            self.prev_hovered = self.hovered;
            self.hover_anim.set_target(if self.hovered.is_some() { 1.0 } else { 0.0 });
        }
    }

    /// Get background color for a row.
    pub fn row_bg(&self, row_idx: usize) -> Color {
        if self.selected == Some(row_idx) {
            self.style.row_selected_bg
        } else if self.hovered == Some(row_idx) {
            self.style.row_hover_bg
        } else if row_idx % 2 == 1 {
            self.style.row_alt_bg
        } else {
            self.style.row_bg
        }
    }

    /// Get foreground color for a row.
    pub fn row_fg(&self, row_idx: usize) -> Color {
        if self.selected == Some(row_idx) {
            self.style.row_selected_fg
        } else {
            self.style.row_fg
        }
    }

    /// Scrollbar thumb position and size (0..1 range).
    pub fn scrollbar(&self) -> (f32, f32) {
        let content = self.rows.len() as f32 * self.style.row_height;
        let viewport = self.viewport_height();
        if content <= viewport {
            return (0.0, 1.0);
        }
        let ratio = viewport / content;
        let position = self.scroll_y.value() / (content - viewport);
        (position * (1.0 - ratio), ratio)
    }
}

/// Events emitted by table interactions.
#[derive(Debug, Clone, PartialEq)]
pub enum TableEvent {
    None,
    Select(usize),
    Activate(usize),
    Sort(usize, bool),
}
