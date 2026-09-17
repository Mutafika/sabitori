//! 時刻ピッカーと日時ピッカー ([#75](https://github.com/Mutafika/sabitori/issues/75) の 1)。
//!
//! [`DatePickerState`](crate::DatePickerState) は年月日しか選べないので、予約・配車の
//! 画面では「日付はカレンダー、時刻は `HH:MM` の文字欄 + 自前検証」になっていた。
//! 打ち間違いの検証も、24 時制の丸めも、営業時間の外を弾くのもアプリ側だった。
//!
//! # 使い方
//!
//! ```ignore
//! // App のフィールド
//! pickup: DateTimePickerState,   // 貸出日時
//!
//! // view()
//! self.pickup.view(ctx.hovered.as_deref(), &DateTimePickerStyle::from_theme(&ctx.theme))
//!
//! // on_click
//! if let Some(dt) = self.pickup.handle_click(id) { /* 選ばれた */ }
//! ```
//!
//! 文字で打たせたい欄には [`parse_hhmm`] を使う — 「9:5」「０９：３０」のような
//! 実際に打たれる形も受ける。

use sabitori_core::element::{div, text, Element, Px, Role};
use sabitori_core::Color;

use crate::date_picker::{DatePickerState, DatePickerStyle};

/// `"HH:MM"` を読む。時刻として成り立たない文字列は `None`。
///
/// 受ける形は業務画面で実際に打たれるもの: `"9:30"` `"09:30"` `"0930"`
/// `"9時30分"` `"０９：３０"` (全角)。区切りは `:` `：` `.` `時` のどれでも。
/// **`"24:00"` は受けない** — 日付をまたぐ表現はアプリの意味づけなので、
/// ピッカーが勝手に翌日 0 時に読み替えない。
pub fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    // 全角を半角に落として、数字と区切りだけ残す。
    let normalized: String = s
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
            '：' | '．' | '時' | '.' => ':',
            '分' => ' ',
            other => other,
        })
        .filter(|c| c.is_ascii_digit() || *c == ':')
        .collect();

    let (h, m) = match normalized.split_once(':') {
        Some((h, m)) => (h.to_string(), m.to_string()),
        // 区切り無し: "0930" / "930" は後ろ 2 桁が分。
        None => {
            if normalized.len() < 3 || normalized.len() > 4 {
                return None;
            }
            let split = normalized.len() - 2;
            (normalized[..split].to_string(), normalized[split..].to_string())
        }
    };
    if h.is_empty() || m.is_empty() || m.len() > 2 {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some((h, m))
}

/// [`TimePickerState::view`] の見た目。
#[derive(Clone, Debug)]
pub struct TimePickerStyle {
    pub bg: Color,
    pub border: Color,
    pub header_fg: Color,
    pub fg: Color,
    pub muted_fg: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub hover_bg: Color,
    pub cell_w: f32,
    pub cell_h: f32,
    pub font_size: f32,
}

impl TimePickerStyle {
    /// [`AppTheme`](sabitori_core::AppTheme) から組む。色はテーマ、寸法は
    /// `default_dark()` と同じ。
    pub fn from_theme(theme: &sabitori_core::AppTheme) -> Self {
        Self {
            bg: theme.surface,
            border: theme.border,
            header_fg: theme.text_primary,
            fg: theme.text_primary,
            muted_fg: theme.text_secondary,
            selected_bg: theme.primary,
            selected_fg: theme.on_primary(),
            hover_bg: theme.hover_bg,
            ..Self::default_dark()
        }
    }

    pub fn default_dark() -> Self {
        Self {
            bg: Color::from_hex("#22223a"),
            border: Color::from_hex("#3a3a55"),
            header_fg: Color::from_hex("#e8e8f0"),
            fg: Color::from_hex("#c8c8dc"),
            muted_fg: Color::from_hex("#9090a8"),
            selected_bg: Color::from_hex("#6c63ff"),
            selected_fg: Color::from_hex("#ffffff"),
            hover_bg: Color::from_hex("#2a2a48"),
            cell_w: 34.0,
            cell_h: 26.0,
            font_size: 12.0,
        }
    }
}

/// 時・分を選ぶ状態。id でクリックを解釈する ([`DatePickerState`] と同じ方式)。
#[derive(Clone, Debug)]
pub struct TimePickerState {
    /// id の接頭辞。内部の id は `"{prefix}:h:{H}"` / `"{prefix}:m:{M}"`。
    prefix: String,
    pub hour: u32,
    pub minute: u32,
    /// 分の刻み (既定 15)。予約の画面で 1 分刻みを出しても押せない。
    minute_step: u32,
    /// 出す時刻の範囲 (両端を含む)。営業時間の外を選ばせないため。
    hour_range: (u32, u32),
}

impl TimePickerState {
    /// `hour` は 0〜23、`minute` は 0〜59 に丸められる。
    pub fn new(id_prefix: impl Into<String>, hour: u32, minute: u32) -> Self {
        Self {
            prefix: id_prefix.into(),
            hour: hour.min(23),
            minute: minute.min(59),
            minute_step: 15,
            hour_range: (0, 23),
        }
    }

    /// 分の刻みを変える (1〜60 に丸める)。
    pub fn with_minute_step(mut self, step: u32) -> Self {
        self.minute_step = step.clamp(1, 60);
        self
    }

    /// 出す時刻の範囲 (営業時間)。`8..=19` なら 8 時から 19 時まで。
    ///
    /// **今の選択が範囲の外なら、範囲の中へ寄せる** — 外のままにすると
    /// 「選ばれているのに一覧に無い」になり、押して直すこともできない。
    pub fn with_hour_range(mut self, from: u32, to: u32) -> Self {
        let from = from.min(23);
        let to = to.min(23).max(from);
        self.hour_range = (from, to);
        self.hour = self.hour.clamp(from, to);
        self
    }

    pub fn selected(&self) -> (u32, u32) {
        (self.hour, self.minute)
    }

    /// `"HH:MM"`。
    pub fn formatted(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// 0 時からの分数。差の計算 (貸出〜返却) に使う。
    pub fn minutes_since_midnight(&self) -> u32 {
        self.hour * 60 + self.minute
    }

    /// 時刻を入れる。範囲の外の時は範囲の中へ寄せる。
    pub fn set_time(&mut self, hour: u32, minute: u32) {
        self.hour = hour.min(23).clamp(self.hour_range.0, self.hour_range.1);
        self.minute = minute.min(59);
    }

    /// 打たれた文字列から入れる。読めなければ `false` で、**今の値は変えない**。
    ///
    /// 読めない入力で 0 時に戻すと、打ちかけの「9:」で時刻が消える。
    pub fn set_from_str(&mut self, s: &str) -> bool {
        match parse_hhmm(s) {
            Some((h, m)) => {
                self.set_time(h, m);
                true
            }
            None => false,
        }
    }

    // ── クリックの解釈 ────────────────────────────────────────────

    /// 押された id を解釈する。時か分が選ばれたら新しい `(時, 分)`。
    pub fn handle_click(&mut self, clicked_id: &str) -> Option<(u32, u32)> {
        let rest = clicked_id.strip_prefix(self.prefix.as_str())?;
        if let Some(h) = rest.strip_prefix(":h:").and_then(|v| v.parse::<u32>().ok()) {
            if h >= self.hour_range.0 && h <= self.hour_range.1 {
                self.hour = h;
                return Some(self.selected());
            }
            return None;
        }
        if let Some(m) = rest.strip_prefix(":m:").and_then(|v| v.parse::<u32>().ok()) {
            if m < 60 && m % self.minute_step == 0 {
                self.minute = m;
                return Some(self.selected());
            }
        }
        None
    }

    // ── 組み立て ──────────────────────────────────────────────────

    /// 時の列と分の列。左が時、右が分。
    pub fn view(&self, hovered: Option<&str>, style: &TimePickerStyle) -> Element {
        let cell = |id: String, label: String, selected: bool| {
            let is_hovered = hovered == Some(id.as_str());
            let (bg, fg) = if selected {
                (style.selected_bg, style.selected_fg)
            } else if is_hovered {
                (style.hover_bg, style.fg)
            } else {
                (Color::TRANSPARENT, style.fg)
            };
            div()
                .id(&id)
                .role(Role::Button)
                .label(&label)
                .w(Px(style.cell_w))
                .h(Px(style.cell_h))
                .bg(bg)
                .rounded_px(4.0)
                .items_center()
                .justify_center()
                .shrink(0.0)
                .child(text(label).font_size(style.font_size).color(fg).shrink(0.0))
        };

        // 時: 4 列ずつ折り返す。
        let hours: Vec<u32> = (self.hour_range.0..=self.hour_range.1).collect();
        let hour_rows: Vec<Element> = hours
            .chunks(4)
            .map(|chunk| {
                div().flex_row().gap(2.0).children(
                    chunk
                        .iter()
                        .map(|&h| {
                            cell(
                                format!("{}:h:{h}", self.prefix),
                                format!("{h:02}"),
                                h == self.hour,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        // 分: 刻みごとに 4 列。
        let minutes: Vec<u32> = (0..60).step_by(self.minute_step as usize).collect();
        let minute_rows: Vec<Element> = minutes
            .chunks(4)
            .map(|chunk| {
                div().flex_row().gap(2.0).children(
                    chunk
                        .iter()
                        .map(|&m| {
                            cell(
                                format!("{}:m:{m}", self.prefix),
                                format!("{m:02}"),
                                m == self.minute,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        let column = |title: &str, rows: Vec<Element>| {
            let mut children = vec![text(title)
                .font_size(style.font_size - 1.0)
                .color(style.muted_fg)
                .shrink(0.0)];
            children.extend(rows);
            div().flex_col().gap(2.0).children(children)
        };

        div()
            .flex_col()
            .gap(6.0)
            .p(Px(8.0))
            .bg(style.bg)
            .border(1.0, style.border)
            .rounded_px(6.0)
            .children([
                text(self.formatted())
                    .font_size(style.font_size + 3.0)
                    .bold()
                    .color(style.header_fg)
                    .shrink(0.0),
                div().flex_row().gap(10.0).children([
                    column("時", hour_rows),
                    column("分", minute_rows),
                ]),
            ])
    }
}

// ---------------------------------------------------------------------------
// 日時ピッカー
// ---------------------------------------------------------------------------

/// [`DateTimePickerState::view`] の見た目 (日付と時刻の両方)。
#[derive(Clone, Debug)]
pub struct DateTimePickerStyle {
    pub date: DatePickerStyle,
    pub time: TimePickerStyle,
    pub gap: f32,
}

impl DateTimePickerStyle {
    pub fn from_theme(theme: &sabitori_core::AppTheme) -> Self {
        Self {
            date: DatePickerStyle::from_theme(theme),
            time: TimePickerStyle::from_theme(theme),
            gap: 8.0,
        }
    }

    pub fn default_dark() -> Self {
        Self {
            date: DatePickerStyle::default_dark(),
            time: TimePickerStyle::default_dark(),
            gap: 8.0,
        }
    }
}

/// カレンダーと時刻を並べた日時ピッカー。
///
/// 内部の id は `"{prefix}:date..."` / `"{prefix}:time..."` なので、同じ画面に
/// 貸出と返却の 2 つを置いてもぶつからない。
pub struct DateTimePickerState {
    pub date: DatePickerState,
    pub time: TimePickerState,
}

impl DateTimePickerState {
    pub fn new(id_prefix: &str, year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Self {
        Self {
            date: DatePickerState::new(format!("{id_prefix}:date"), year, month, day),
            time: TimePickerState::new(format!("{id_prefix}:time"), hour, minute),
        }
    }

    /// 分の刻みを変える。
    pub fn with_minute_step(mut self, step: u32) -> Self {
        self.time = self.time.with_minute_step(step);
        self
    }

    /// 営業時間。
    pub fn with_hour_range(mut self, from: u32, to: u32) -> Self {
        self.time = self.time.with_hour_range(from, to);
        self
    }

    /// `(年, 月, 日, 時, 分)`。
    pub fn selected(&self) -> (i32, u32, u32, u32, u32) {
        let (y, mo, d) = self.date.selected();
        let (h, mi) = self.time.selected();
        (y, mo, d, h, mi)
    }

    /// `"YYYY-MM-DD HH:MM"`。
    pub fn formatted(&self) -> String {
        format!("{} {}", self.date.formatted(), self.time.formatted())
    }

    /// **並べ替えや期間の比較に使える数**。日付と時刻を分で通した値。
    ///
    /// 「貸出より前の返却」を弾くのに、文字列を組み立てて比べなくてよい。
    pub fn as_minutes(&self) -> i64 {
        let (y, m, d) = self.date.selected();
        let days = days_from_civil(y, m, d);
        days * 24 * 60 + self.time.minutes_since_midnight() as i64
    }

    /// 日付側・時刻側のどちらのクリックも解釈する。
    pub fn handle_click(&mut self, clicked_id: &str) -> Option<(i32, u32, u32, u32, u32)> {
        if self.date.handle_click(clicked_id).is_some() || self.time.handle_click(clicked_id).is_some() {
            return Some(self.selected());
        }
        // 月送り (◀ / ▶) は選択を変えないので `None` のまま。
        None
    }

    pub fn view(&self, hovered: Option<&str>, style: &DateTimePickerStyle) -> Element {
        div().flex_row().gap(style.gap).items_start().children([
            self.date.view(hovered, &style.date),
            self.time.view(hovered, &style.time),
        ])
    }
}

/// グレゴリオ暦の日付 → 1970-01-01 からの日数 (Howard Hinnant の `days_from_civil`)。
///
/// 期間の比較のために要る。`chrono` を引き込まないのは、暦の計算がこの 10 行で
/// 足りるのに依存を 1 本増やしたくないから (日付ピッカーも同じ判断)。
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hhmm_accepts_what_people_actually_type() {
        assert_eq!(parse_hhmm("9:30"), Some((9, 30)));
        assert_eq!(parse_hhmm("09:30"), Some((9, 30)));
        assert_eq!(parse_hhmm("0930"), Some((9, 30)));
        assert_eq!(parse_hhmm("930"), Some((9, 30)));
        assert_eq!(parse_hhmm("9時30分"), Some((9, 30)));
        assert_eq!(parse_hhmm("０９：３０"), Some((9, 30)), "全角");
        assert_eq!(parse_hhmm("9.30"), Some((9, 30)));
        assert_eq!(parse_hhmm("23:59"), Some((23, 59)));
        assert_eq!(parse_hhmm("0:00"), Some((0, 0)));
    }

    #[test]
    fn hhmm_rejects_what_is_not_a_time() {
        assert_eq!(parse_hhmm(""), None);
        assert_eq!(parse_hhmm("9"), None, "打ちかけ");
        assert_eq!(parse_hhmm("9:"), None);
        assert_eq!(parse_hhmm("24:00"), None, "日付をまたぐ表現は受けない");
        assert_eq!(parse_hhmm("9:60"), None);
        assert_eq!(parse_hhmm("99:99"), None);
        assert_eq!(parse_hhmm("あした"), None);
    }

    /// **読めない入力で今の値を消さない。** 打ちかけの「9:」で時刻が
    /// 0 時に戻ると、打ち終える前に消える。
    #[test]
    fn a_half_typed_time_leaves_the_value_alone() {
        let mut t = TimePickerState::new("t", 9, 30);
        assert!(!t.set_from_str("9:"));
        assert_eq!(t.selected(), (9, 30));
        assert!(t.set_from_str("10:45"));
        assert_eq!(t.selected(), (10, 45));
    }

    #[test]
    fn clicking_an_hour_and_a_minute_selects_them() {
        let mut t = TimePickerState::new("pick", 9, 0);
        assert_eq!(t.handle_click("pick:h:14"), Some((14, 0)));
        assert_eq!(t.handle_click("pick:m:30"), Some((14, 30)));
        assert_eq!(t.formatted(), "14:30");
    }

    /// 刻みに乗らない分は押せない (出していないものが選ばれない)。
    #[test]
    fn a_minute_off_the_step_is_ignored() {
        let mut t = TimePickerState::new("pick", 9, 0).with_minute_step(15);
        assert_eq!(t.handle_click("pick:m:7"), None);
        assert_eq!(t.selected(), (9, 0));
        assert_eq!(t.handle_click("pick:m:45"), Some((9, 45)));
    }

    /// 営業時間の外は押せず、**選択も範囲の中へ寄る**。
    #[test]
    fn a_business_hour_range_clamps_and_rejects() {
        let mut t = TimePickerState::new("pick", 3, 0).with_hour_range(8, 19);
        assert_eq!(t.hour, 8, "範囲の外のまま始まっている");
        assert_eq!(t.handle_click("pick:h:22"), None);
        assert_eq!(t.handle_click("pick:h:19"), Some((19, 0)));
    }

    #[test]
    fn unrelated_ids_are_ignored() {
        let mut t = TimePickerState::new("pick", 9, 0);
        assert_eq!(t.handle_click("other:h:10"), None);
        assert_eq!(t.handle_click("pick:nonsense"), None);
    }

    /// 出ているセルが、押せる id と一致すること。
    #[test]
    fn the_view_shows_exactly_the_cells_that_can_be_clicked() {
        let t = TimePickerState::new("pick", 9, 0)
            .with_hour_range(8, 10)
            .with_minute_step(30);
        let el = t.view(None, &TimePickerStyle::default_dark());
        let mut ids = Vec::new();
        fn walk(e: &Element, out: &mut Vec<String>) {
            if let Some(id) = &e.id {
                out.push(id.clone());
            }
            for c in &e.children {
                walk(c, out);
            }
        }
        walk(&el, &mut ids);
        assert!(ids.iter().any(|i| i == "pick:h:8"));
        assert!(ids.iter().any(|i| i == "pick:h:10"));
        assert!(!ids.iter().any(|i| i == "pick:h:11"), "範囲の外が出ている");
        assert!(ids.iter().any(|i| i == "pick:m:30"));
        assert!(!ids.iter().any(|i| i == "pick:m:15"), "刻みの外が出ている");
    }

    // ── 日時 ─────────────────────────────────────────────────────

    #[test]
    fn a_datetime_picker_routes_both_halves() {
        let mut dt = DateTimePickerState::new("pickup", 2026, 6, 10, 9, 0);
        assert_eq!(dt.formatted(), "2026-06-10 09:00");

        assert!(dt.handle_click("pickup:date:day:15").is_some());
        assert!(dt.handle_click("pickup:time:h:13").is_some());
        assert_eq!(dt.formatted(), "2026-06-15 13:00");
    }

    /// **期間の比較が数でできる。** 「貸出より前の返却」を弾くのに
    /// 文字列を組み立てて比べなくてよい。
    #[test]
    fn two_datetimes_compare_as_numbers() {
        let out = DateTimePickerState::new("out", 2026, 6, 10, 9, 0);
        let back = DateTimePickerState::new("back", 2026, 6, 12, 8, 30);
        assert!(back.as_minutes() > out.as_minutes());
        assert_eq!(back.as_minutes() - out.as_minutes(), 2 * 24 * 60 - 30);

        // 年またぎ。
        let dec = DateTimePickerState::new("a", 2026, 12, 31, 23, 0);
        let jan = DateTimePickerState::new("b", 2027, 1, 1, 1, 0);
        assert_eq!(jan.as_minutes() - dec.as_minutes(), 120);
    }

    /// 2 つ置いても id がぶつからない。
    #[test]
    fn two_pickers_on_one_screen_do_not_collide() {
        let mut out = DateTimePickerState::new("out", 2026, 6, 10, 9, 0);
        let mut back = DateTimePickerState::new("back", 2026, 6, 10, 9, 0);
        assert!(out.handle_click("back:time:h:15").is_none(), "他方の id を拾っている");
        assert!(back.handle_click("back:time:h:15").is_some());
        assert_eq!(out.time.hour, 9);
    }
}
