//! Simple calendar date picker — 工程表・タイムライン向け.
//!
//! 年月ヘッダ（◀ / ▶ で月送り）とカレンダー格子で年月日を選ぶ簡易版。
//! 外部クレート (chrono 等) には依存せず、 グレゴリオ暦の閏年・曜日計算
//! （Sakamoto 法）を内蔵する。
//!
//! State and visuals are split, sabitori 流: [`DatePickerState`] owns the
//! selected date and the displayed month, and interprets clicks by
//! element id; [`DatePickerState::view`] builds the calendar panel.
//!
//! ## Wiring
//!
//! ```ignore
//! // view():
//! picker.view(hovered, &DatePickerStyle::default_dark())
//! // on_click:
//! if let Some((y, m, d)) = picker.handle_click(id) {
//!     // 日付セルが押された (◀/▶ は内部で月送りして None)
//! }
//! ```

use sabitori_core::element::{div, text, Element, Px};
use sabitori_core::Color;

/// Visual parameters for [`DatePickerState::view`].
#[derive(Clone, Debug)]
pub struct DatePickerStyle {
    pub bg: Color,
    pub border: Color,
    pub header_fg: Color,
    pub weekday_fg: Color,
    pub day_fg: Color,
    /// Foreground for days outside the current month grid (unused cells).
    pub muted_fg: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub hover_bg: Color,
    pub cell_size: f32,
    pub font_size: f32,
}

impl DatePickerStyle {
    pub fn default_dark() -> Self {
        Self {
            bg: Color::from_hex("#22223a"),
            border: Color::from_hex("#3a3a55"),
            header_fg: Color::from_hex("#e8e8f0"),
            weekday_fg: Color::from_hex("#9090a8"),
            day_fg: Color::from_hex("#c8c8dc"),
            muted_fg: Color::from_hex("#55556a"),
            selected_bg: Color::from_hex("#6c63ff"),
            selected_fg: Color::from_hex("#ffffff"),
            hover_bg: Color::from_hex("#2a2a48"),
            cell_size: 26.0,
            font_size: 12.0,
        }
    }
}

/// State for the calendar date picker.
pub struct DatePickerState {
    /// Element-id prefix; internal ids are `"{prefix}:prev"`,
    /// `"{prefix}:next"`, `"{prefix}:day:{d}"`.
    prefix: String,
    /// Selected date.
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Displayed (visible) month — navigated with ◀ / ▶.
    pub view_year: i32,
    pub view_month: u32,
}

impl DatePickerState {
    /// `month` is 1–12, `day` is clamped to the month's length.
    pub fn new(id_prefix: impl Into<String>, year: i32, month: u32, day: u32) -> Self {
        let month = month.clamp(1, 12);
        let day = day.clamp(1, days_in_month(year, month));
        Self {
            prefix: id_prefix.into(),
            year,
            month,
            day,
            view_year: year,
            view_month: month,
        }
    }

    /// Selected date as `(year, month, day)`.
    pub fn selected(&self) -> (i32, u32, u32) {
        (self.year, self.month, self.day)
    }

    /// Selected date as `"YYYY-MM-DD"`.
    pub fn formatted(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Set the selected date (clamps the day) and jump the view to it.
    pub fn set_date(&mut self, year: i32, month: u32, day: u32) {
        let month = month.clamp(1, 12);
        self.year = year;
        self.month = month;
        self.day = day.clamp(1, days_in_month(year, month));
        self.view_year = year;
        self.view_month = month;
    }

    /// Show the previous month (no selection change).
    pub fn prev_month(&mut self) {
        if self.view_month == 1 {
            self.view_month = 12;
            self.view_year -= 1;
        } else {
            self.view_month -= 1;
        }
    }

    /// Show the next month (no selection change).
    pub fn next_month(&mut self) {
        if self.view_month == 12 {
            self.view_month = 1;
            self.view_year += 1;
        } else {
            self.view_month += 1;
        }
    }

    // ── Event handling ────────────────────────────────────────────

    /// Interpret a click by element id. Month navigation is handled
    /// internally (returns `None`); a day-cell click selects that day in
    /// the displayed month and returns the new `(year, month, day)`.
    /// Unrelated ids return `None`.
    pub fn handle_click(&mut self, clicked_id: &str) -> Option<(i32, u32, u32)> {
        let rest = clicked_id.strip_prefix(self.prefix.as_str())?;
        match rest {
            ":prev" => {
                self.prev_month();
                None
            }
            ":next" => {
                self.next_month();
                None
            }
            _ => {
                let d: u32 = rest.strip_prefix(":day:")?.parse().ok()?;
                if d >= 1 && d <= days_in_month(self.view_year, self.view_month) {
                    self.year = self.view_year;
                    self.month = self.view_month;
                    self.day = d;
                    Some(self.selected())
                } else {
                    None
                }
            }
        }
    }

    // ── Element builder ───────────────────────────────────────────

    /// Build the calendar panel: header (◀ 2026年 6月 ▶), weekday row,
    /// day grid (Sunday-first).
    pub fn view(&self, hovered: Option<&str>, style: &DatePickerStyle) -> Element {
        let cell = style.cell_size;
        let grid_w = cell * 7.0;

        // Header: ◀ | 2026年 6月 | ▶
        let nav = |id: String, label: &str| {
            let bg = if hovered == Some(id.as_str()) {
                style.hover_bg
            } else {
                Color::TRANSPARENT
            };
            div()
                .id(&id)
                .w(Px(cell))
                .h(Px(cell))
                .bg(bg)
                .rounded_px(4.0)
                .items_center()
                .justify_center()
                .shrink(0.0)
                .child(
                    text(label)
                        .font_size(style.font_size)
                        .color(style.header_fg)
                        .shrink(0.0),
                )
        };
        let header = div()
            .w(Px(grid_w))
            .flex_row()
            .items_center()
            .justify_between()
            .children([
                nav(format!("{}:prev", self.prefix), "\u{25C0}"), // ◀
                text(&format!("{}年 {}月", self.view_year, self.view_month))
                    .font_size(style.font_size + 1.0)
                    .bold()
                    .color(style.header_fg)
                    .shrink(0.0),
                nav(format!("{}:next", self.prefix), "\u{25B6}"), // ▶
            ]);

        // Weekday row (Sunday-first, JP convention).
        let weekdays = ["日", "月", "火", "水", "木", "金", "土"];
        let weekday_row = div().flex_row().children(
            weekdays
                .iter()
                .map(|w| {
                    div()
                        .w(Px(cell))
                        .h(Px(cell * 0.8))
                        .items_center()
                        .justify_center()
                        .shrink(0.0)
                        .child(
                            text(*w)
                                .font_size(style.font_size - 1.0)
                                .color(style.weekday_fg)
                                .shrink(0.0),
                        )
                })
                .collect::<Vec<_>>(),
        );

        // Day grid.
        let first_wd = weekday(self.view_year, self.view_month, 1);
        let n_days = days_in_month(self.view_year, self.view_month);
        let selected_visible =
            self.view_year == self.year && self.view_month == self.month;

        let mut rows: Vec<Element> = Vec::new();
        let mut current: Vec<Element> = Vec::new();
        // Leading blanks before day 1.
        for _ in 0..first_wd {
            current.push(div().w(Px(cell)).h(Px(cell)).shrink(0.0));
        }
        for d in 1..=n_days {
            let id = format!("{}:day:{d}", self.prefix);
            let is_selected = selected_visible && d == self.day;
            let is_hovered = hovered == Some(id.as_str());
            let (bg, fg) = if is_selected {
                (style.selected_bg, style.selected_fg)
            } else if is_hovered {
                (style.hover_bg, style.day_fg)
            } else {
                (Color::TRANSPARENT, style.day_fg)
            };
            current.push(
                div()
                    .id(&id)
                    .w(Px(cell))
                    .h(Px(cell))
                    .bg(bg)
                    .rounded_px(4.0)
                    .items_center()
                    .justify_center()
                    .shrink(0.0)
                    .child(
                        text(&d.to_string())
                            .font_size(style.font_size)
                            .color(fg)
                            .shrink(0.0),
                    ),
            );
            if current.len() == 7 {
                rows.push(div().flex_row().children(std::mem::take(&mut current)));
            }
        }
        if !current.is_empty() {
            // Trailing blanks to keep row width stable.
            while current.len() < 7 {
                current.push(div().w(Px(cell)).h(Px(cell)).shrink(0.0));
            }
            rows.push(div().flex_row().children(current));
        }

        let mut children = vec![header, weekday_row];
        children.extend(rows);

        div()
            .flex_col()
            .gap(2.0)
            .p(Px(8.0))
            .bg(style.bg)
            .border(1.0, style.border)
            .rounded_px(6.0)
            .children(children)
    }
}

// ── Gregorian calendar math ───────────────────────────────────────

/// Proleptic Gregorian leap-year rule.
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Number of days in `month` (1–12) of `year`.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30, // out-of-range months are clamped upstream
    }
}

/// Day of week for a Gregorian date (0 = Sunday … 6 = Saturday).
/// Sakamoto's method.
pub fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let m = month.clamp(1, 12) as usize;
    let y = if m < 3 { year - 1 } else { year };
    let w = y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)
        + T[m - 1]
        + day as i32;
    w.rem_euclid(7) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2026));
        assert!(!is_leap_year(1900), "century rule");
        assert!(is_leap_year(2000), "400-year rule");
    }

    #[test]
    fn month_lengths() {
        assert_eq!(days_in_month(2026, 6), 30);
        assert_eq!(days_in_month(2026, 7), 31);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
    }

    #[test]
    fn weekday_known_dates() {
        assert_eq!(weekday(2026, 6, 10), 3, "2026-06-10 is Wednesday");
        assert_eq!(weekday(2000, 1, 1), 6, "2000-01-01 is Saturday");
        assert_eq!(weekday(2024, 2, 29), 4, "2024-02-29 is Thursday");
        assert_eq!(weekday(1970, 1, 1), 4, "Unix epoch is Thursday");
    }

    #[test]
    fn day_click_selects() {
        let mut p = DatePickerState::new("dp", 2026, 6, 10);
        assert_eq!(p.handle_click("dp:day:15"), Some((2026, 6, 15)));
        assert_eq!(p.selected(), (2026, 6, 15));
        assert_eq!(p.formatted(), "2026-06-15");
    }

    #[test]
    fn out_of_range_day_ignored() {
        let mut p = DatePickerState::new("dp", 2026, 6, 10);
        assert_eq!(p.handle_click("dp:day:31"), None, "June has 30 days");
        assert_eq!(p.handle_click("dp:day:0"), None);
        assert_eq!(p.selected(), (2026, 6, 10));
    }

    #[test]
    fn month_nav_wraps_year_and_keeps_selection() {
        let mut p = DatePickerState::new("dp", 2026, 1, 5);
        assert_eq!(p.handle_click("dp:prev"), None);
        assert_eq!((p.view_year, p.view_month), (2025, 12));
        assert_eq!(p.selected(), (2026, 1, 5), "selection unchanged by nav");

        p.handle_click("dp:next");
        p.handle_click("dp:next");
        assert_eq!((p.view_year, p.view_month), (2026, 2));

        // Selecting in the navigated month moves the selection there.
        assert_eq!(p.handle_click("dp:day:28"), Some((2026, 2, 28)));
    }

    #[test]
    fn unrelated_ids_ignored() {
        let mut p = DatePickerState::new("dp", 2026, 6, 10);
        assert_eq!(p.handle_click("other:day:5"), None);
        assert_eq!(p.handle_click("dp:nonsense"), None);
    }

    #[test]
    fn new_clamps_day() {
        let p = DatePickerState::new("dp", 2026, 2, 31);
        assert_eq!(p.selected(), (2026, 2, 28));
        let p = DatePickerState::new("dp", 2026, 99, 1);
        assert_eq!(p.month, 12);
    }

    #[test]
    fn set_date_jumps_view() {
        let mut p = DatePickerState::new("dp", 2026, 6, 10);
        p.next_month();
        p.set_date(2027, 3, 31);
        assert_eq!((p.view_year, p.view_month), (2027, 3));
        assert_eq!(p.selected(), (2027, 3, 31));
    }

    #[test]
    fn view_grid_has_correct_day_cells_and_offset() {
        let p = DatePickerState::new("dp", 2026, 6, 10);
        let el = p.view(None, &DatePickerStyle::default_dark());
        let mut ids = Vec::new();
        fn walk(e: &Element, out: &mut Vec<String>) {
            if let Some(ref id) = e.id {
                out.push(id.clone());
            }
            for c in &e.children {
                walk(c, out);
            }
        }
        walk(&el, &mut ids);
        assert!(ids.iter().any(|i| i == "dp:prev"));
        assert!(ids.iter().any(|i| i == "dp:next"));
        assert!(ids.iter().any(|i| i == "dp:day:1"));
        assert!(ids.iter().any(|i| i == "dp:day:30"));
        assert!(!ids.iter().any(|i| i == "dp:day:31"), "June has 30 days");

        // 2026-06-01 is a Monday → first row starts with 1 blank.
        assert_eq!(weekday(2026, 6, 1), 1);
    }
}
