//! **タッチで文字を選ぶ** ([#108](https://github.com/Mutafika/sabitori/issues/108))。
//!
//! マウスの選択はドラッグが起点だが、タッチのドラッグはスクロールに使う。iOS の
//! 標準に倣って:
//!
//! - **長押しで語をつかむ** — 指を [`LONG_PRESS_SECS`] 秒、[`TOUCH_SLOP`] 以内に
//!   留めたら、その下の語を選ぶ。そのまま指を動かすと範囲が伸びる
//! - **両端のつまみ**で広げる・縮める。選択が指で作られたときだけ描く
//! - **選択の外をタップしたら解除** (タップした先の `on_click` は先に届く)
//!
//! メニュー (コピー・マーカー…) は持たない。アプリが `on_selection_changed` と
//! [`SelectedText::bounds`](sabitori_core::SelectedText::bounds) で、選択の近くに
//! 自前のものを出す。
//!
//! ここは判定と形だけを持つ (窓もランタイムも要らない)。つなぎ込みは
//! `declarative.rs` の `handle_touch` / `advance`。
//!
//! [`TOUCH_SLOP`]: crate::input_router::TOUCH_SLOP

use crate::bridge::TextHitLayout;

/// 長押しとみなす時間 (秒)。iOS の既定 (0.5 秒) と同じ。
pub(crate) const LONG_PRESS_SECS: f32 = 0.5;
/// つまみの丸の半径 (論理 px)。
pub(crate) const HANDLE_RADIUS: f32 = 6.0;
/// つまみをつかめる距離 (論理 px)。丸は小さいので、指の太さぶん広く取る。
pub(crate) const HANDLE_GRAB: f32 = 24.0;

/// 指で選んでいる最中の状態。`TouchDrag` に載る。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TouchSelect {
    /// 長押しで語をつかんだ。指を動かすと、この語を含んだまま伸びる。
    Word { idx: usize, start: usize, end: usize },
    /// つまみを動かしている。`fixed` は動かさない側の端。`dy` は指から端の行の
    /// 中ほどまでの縦のずれ — つまみの丸は行の上 (下) に出ているので、指の位置で
    /// そのまま文字を探すと隣の行をつかむ。
    Handle { fixed: (usize, usize), dy: f32 },
}

/// 文字の種類。同じ種類が続く所を 1 語とみなす。
///
/// UAX #29 の語の区切りは漢字を 1 字ずつに切る (「損害賠償」が 4 語) ので使わない。
/// 日本語の辞書は持たないので、漢字・ひらがな・カタカナ・英数の**続き**で区切る:
/// 「損害賠償の責任」を長押しすると「損害賠償」。
fn class(c: char) -> u8 {
    match c {
        c if c.is_whitespace() => 0,
        '\u{3041}'..='\u{309F}' => 1,
        '\u{30A0}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9F}' => 2,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '\u{3005}' => 3,
        c if c.is_alphanumeric() || c == '_' => 4,
        _ => 5,
    }
}

/// `byte` の位置の語のバイト範囲。空白・記号は 1 字だけ。
pub(crate) fn word_range(content: &str, byte: usize) -> (usize, usize) {
    if content.is_empty() {
        return (0, 0);
    }
    let mut at = byte.min(content.len());
    while at > 0 && !content.is_char_boundary(at) {
        at -= 1;
    }
    // 末尾 (最後の字の右半分を押した) なら、最後の字の語。
    if at == content.len() {
        at = content[..at].char_indices().last().map_or(0, |(i, _)| i);
    }
    let c = content[at..].chars().next().unwrap_or(' ');
    let k = class(c);
    if k == 0 || k == 5 {
        return (at, at + c.len_utf8());
    }
    let mut start = at;
    for (i, ch) in content[..at].char_indices().rev() {
        if class(ch) != k {
            break;
        }
        start = i;
    }
    let mut end = at;
    for (i, ch) in content[at..].char_indices() {
        if class(ch) != k {
            break;
        }
        end = at + i + ch.len_utf8();
    }
    (start, end)
}

/// 選択の端の位置 `(x, 行の上端, 行の高さ)`。`end` なら字の右端 (行末の端が次の行の
/// 頭へ飛ばないように)、そうでなければ字の左端。
fn caret(layout: &TextHitLayout, byte: usize, end: bool) -> Option<(f32, f32, f32)> {
    let left = |b: usize| layout.hits.iter().find(|h| h.byte_start == b).map(|h| (h.x, h.y, h.h));
    let right = |b: usize| layout.hits.iter().find(|h| h.byte_end == b).map(|h| (h.x + h.w, h.y, h.h));
    if end { right(byte).or_else(|| left(byte)) } else { left(byte).or_else(|| right(byte)) }
}

/// 選択の両端 `[始まり, 終わり]` の位置 (`(x, 行の上端, 行の高さ)`)。
pub(crate) fn ends(
    start: (usize, usize),
    end: (usize, usize),
    layouts: &[TextHitLayout],
) -> Option<[(f32, f32, f32); 2]> {
    let find = |idx: usize| layouts.iter().find(|l| l.text_idx == idx);
    let a = caret(find(start.0)?, start.1, false)?;
    let b = caret(find(end.0)?, end.1, true)?;
    Some([a, b])
}

/// つまみの丸の中心。始まりは行の上に、終わりは行の下に出す (iOS と同じ向き)。
pub(crate) fn knob_centers(ends: [(f32, f32, f32); 2]) -> [(f32, f32); 2] {
    let [(x0, y0, _), (x1, y1, h1)] = ends;
    [(x0, y0 - HANDLE_RADIUS), (x1, y1 + h1 + HANDLE_RADIUS)]
}

/// 押した所がつまみなら、どちらか (0 = 始まり, 1 = 終わり)。近い方。
pub(crate) fn grabbed(ends: [(f32, f32, f32); 2], x: f32, y: f32) -> Option<usize> {
    let d = |(cx, cy): (f32, f32)| ((cx - x).powi(2) + (cy - y).powi(2)).sqrt();
    let [a, b] = knob_centers(ends);
    let (da, db) = (d(a), d(b));
    match (da <= HANDLE_GRAB, db <= HANDLE_GRAB) {
        (true, true) => Some(if da <= db { 0 } else { 1 }),
        (true, false) => Some(0),
        (false, true) => Some(1),
        _ => None,
    }
}

/// つまみの形: 端に沿った細い棒 + 丸。
pub(crate) fn handle_rects(
    ends: [(f32, f32, f32); 2],
    color: sabitori_core::Color,
) -> Vec<sabitori_gpu::RectInstance> {
    use sabitori_core::render_list::RectDraw;
    use sabitori_core::{Corners, Rect};
    let fill = |rect: Rect, r: f32| {
        crate::bridge::rect_to_instance(&RectDraw {
            rect,
            corner_radii: Corners::all(r),
            fill_color: color,
            ..Default::default()
        })
    };
    let mut out = Vec::with_capacity(4);
    for (&(x, y, h), (cx, cy)) in ends.iter().zip(knob_centers(ends)) {
        out.push(fill(Rect::new(x - 1.0, y, 2.0, h), 0.0));
        let r = HANDLE_RADIUS;
        out.push(fill(Rect::new(cx - r, cy - r, r * 2.0, r * 2.0), r));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_text::GlyphHit;

    #[test]
    fn a_word_is_a_run_of_one_kind_of_character() {
        let s = "損害賠償の責任を負う";
        // 「賠」を押すと「損害賠償」。
        assert_eq!(&s[{
            let (a, b) = word_range(s, "損害".len());
            a..b
        }], "損害賠償");
        let (a, b) = word_range(s, "損害賠償の責".len());
        assert_eq!(&s[a..b], "責任");
        let (a, b) = word_range(s, "損害賠償".len());
        assert_eq!(&s[a..b], "の");
        let t = "Hello, world_2 アイコン";
        let (a, b) = word_range(t, 2);
        assert_eq!(&t[a..b], "Hello");
        let (a, b) = word_range(t, 5);
        assert_eq!(&t[a..b], ",", "記号は 1 字");
        let (a, b) = word_range(t, 9);
        assert_eq!(&t[a..b], "world_2");
        let (a, b) = word_range(t, t.len());
        assert_eq!(&t[a..b], "アイコン", "末尾は最後の語");
        assert_eq!(word_range("", 0), (0, 0));
    }

    fn line(idx: usize, content: &str, y: f32) -> TextHitLayout {
        TextHitLayout {
            text_idx: idx,
            content: content.into(),
            hits: content
                .char_indices()
                .enumerate()
                .map(|(i, (b, c))| GlyphHit {
                    byte_start: b,
                    byte_end: b + c.len_utf8(),
                    x: i as f32 * 10.0,
                    y,
                    w: 10.0,
                    h: 16.0,
                    line_index: 0,
                })
                .collect(),
            clip_rect: None,
            highlight: Vec::new(),
            link_ranges: None,
            no_select: false,
            owner: None,
        }
    }

    #[test]
    fn the_handles_sit_at_both_ends_and_can_be_grabbed() {
        let layouts = [line(0, "abcdef", 0.0), line(1, "ghij", 40.0)];
        let e = ends((0, 2), (1, 3), &layouts).unwrap();
        assert_eq!(e, [(20.0, 0.0, 16.0), (30.0, 40.0, 16.0)]);
        assert_eq!(knob_centers(e), [(20.0, -6.0), (30.0, 62.0)]);
        assert_eq!(grabbed(e, 22.0, 0.0), Some(0));
        assert_eq!(grabbed(e, 30.0, 70.0), Some(1));
        assert_eq!(grabbed(e, 100.0, 20.0), None);
        // 行末で終わる選択の端は、行の右端 (次の行の頭ではない)。
        let e = ends((0, 0), (0, 6), &layouts).unwrap();
        assert_eq!(e[1].0, 60.0);
        assert_eq!(handle_rects(e, sabitori_core::Color::WHITE).len(), 4);
    }
}
