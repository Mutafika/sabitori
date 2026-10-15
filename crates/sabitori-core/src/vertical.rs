//! 縦書き ([`crate::Element::vertical`])。
//!
//! 字は 1 字ずつ升目 (1 字の高さ × 列の幅) に置く。列は右から左へ並び、改行か、箱の
//! 高さに字が収まらなくなった所で次の列へ送る。
//!
//! 横書きの字形のまま縦に並べると、句読点や括弧が升目の左下に落ちる。そこで
//! - 句読点は、升目の右上へ動かす。
//! - 括弧・ダッシュ・三点リーダー・長音記号・波線・半角の英数字は、升目の真ん中で
//!   90° 回す (括弧は回すと縦書きの形になる)。
//! - 小さい仮名 (ゃ・っ など) は、升目の右上へ少し寄せる。
//!
//! ここは字の置き場所を決めるだけ (どの書体でも同じ)。字形を作って描くのは
//! `sabitori_text::TextRenderer::prepare_vertical`。

use crate::element::Typography;

/// 1 字を升目にどう置くか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalPlacement {
    /// そのまま (縦書き形に替えた字も含む)。
    Upright,
    /// 升目の真ん中で時計回りに 90° 回す。
    Rotated,
    /// 小さい仮名。右上へ少し寄せる。
    Small,
    /// 句読点。升目の左下にある横書きの形を、右上へ動かす。
    Punct,
}

/// 縦書きで描く字と、その置き方。
///
/// 書体の縦書き用の字形 (Unicode の縦書き形) には頼らない — 持っていない書体が多く、
/// 落ちた先の書体では横書きと同じ形のまま描かれる。横書きの字形を、句読点は升目の
/// 右上へ動かし、括弧・ダッシュ・三点リーダーは 90° 回して縦の形にする。
pub fn vertical_form(c: char) -> (char, VerticalPlacement) {
    use VerticalPlacement::*;
    match c {
        '、' | '。' | '，' | '．' => (c, Punct),
        '「' | '」' | '『' | '』' | '（' | '）' | '(' | ')' | '［' | '］' | '[' | ']' | '｛' | '｝' | '〔' | '〕' | '【'
        | '】' | '《' | '》' | '〈' | '〉' | '〖' | '〗' | '…' | '‥' | '—' | '―' | 'ー' | '～' | '〜' | '－' | '＝'
        | '：' | '；' => (c, Rotated),
        'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ' | 'ゕ' | 'ゖ' | 'ァ' | 'ィ' | 'ゥ'
        | 'ェ' | 'ォ' | 'ッ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ' | 'ヵ' | 'ヶ' => (c, Small),
        c if c.is_ascii_graphic() => (c, Rotated),
        c => (c, Upright),
    }
}

/// 縦書きの升目の大きさ: (1 字の送り, 列の送り)。
pub fn vertical_pitch(font_size: f32, typo: &Typography) -> (f32, f32) {
    let step = (font_size + typo.letter_spacing).max(1.0);
    let column = font_size * typo.line_height.unwrap_or(1.6);
    (step, column.max(1.0))
}

/// 1 字の置き場所 (升目の列と行、本文のバイト範囲、描く字と置き方)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticalCell {
    pub column: usize,
    pub row: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub form: char,
    pub placement: VerticalPlacement,
}

/// 1 列に入る字数。`max_height` が無ければ限り無し。
fn rows_per_column(step: f32, max_height: Option<f32>) -> usize {
    match max_height {
        Some(h) if h.is_finite() => ((h + 0.5) / step).floor().max(1.0) as usize,
        _ => usize::MAX,
    }
}

/// 字を升目に割り振る。改行で次の列へ、`max_height` を超える所でも次の列へ送る。
pub fn vertical_cells(content: &str, font_size: f32, typo: &Typography, max_height: Option<f32>) -> Vec<VerticalCell> {
    let (step, _) = vertical_pitch(font_size, typo);
    let rows = rows_per_column(step, max_height);
    let mut out = Vec::new();
    let (mut col, mut row) = (0usize, 0usize);
    for (b, c) in content.char_indices() {
        if c == '\n' {
            col += 1;
            row = 0;
            continue;
        }
        if row >= rows {
            col += 1;
            row = 0;
        }
        let (form, placement) = vertical_form(c);
        out.push(VerticalCell { column: col, row, byte_start: b, byte_end: b + c.len_utf8(), form, placement });
        row += 1;
    }
    out
}

/// 縦書きの文字の大きさ (幅, 高さ)。列は右から左に並ぶので、幅は列の数で決まる。
pub fn vertical_extent(content: &str, font_size: f32, typo: &Typography, max_height: Option<f32>) -> (f32, f32) {
    let (step, column) = vertical_pitch(font_size, typo);
    let cells = vertical_cells(content, font_size, typo, max_height);
    let columns = content.chars().filter(|&c| c == '\n').count() + 1;
    let columns = columns.max(cells.last().map_or(0, |c| c.column + 1));
    let rows = cells.iter().map(|c| c.row + 1).max().unwrap_or(0);
    // 最後の列は字の幅だけ (列の間の空きは要らない)。
    let width = (columns.saturating_sub(1)) as f32 * column + font_size;
    (width, rows as f32 * step)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typo() -> Typography {
        Typography { line_height: Some(1.5), ..Default::default() }
    }

    #[test]
    fn punctuation_moves_and_brackets_turn() {
        assert_eq!(vertical_form('、'), ('、', VerticalPlacement::Punct));
        assert_eq!(vertical_form('「'), ('「', VerticalPlacement::Rotated));
        assert_eq!(vertical_form('ー').1, VerticalPlacement::Rotated);
        assert_eq!(vertical_form('A').1, VerticalPlacement::Rotated);
        assert_eq!(vertical_form('ょ').1, VerticalPlacement::Small);
        assert_eq!(vertical_form('河'), ('河', VerticalPlacement::Upright));
    }

    /// 改行で次の列、高さに収まらない所でも次の列。バイト範囲は本文のまま。
    #[test]
    fn cells_wrap_into_columns() {
        let cells = vertical_cells("あいう\nかきくけこ", 20.0, &typo(), Some(60.0));
        let pos: Vec<(usize, usize)> = cells.iter().map(|c| (c.column, c.row)).collect();
        assert_eq!(pos, [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (2, 0), (2, 1)]);
        assert_eq!((cells[3].byte_start, cells[3].byte_end), (10, 13));
    }

    #[test]
    fn extent_counts_columns_and_rows() {
        // 3 列 (1.5 × 20 の送り、最後は字の幅だけ)、3 字の高さ。
        assert_eq!(vertical_extent("あいう\nかきくけこ", 20.0, &typo(), Some(60.0)), (80.0, 60.0));
        // 高さの限りが無ければ、段落ごとに 1 列。
        assert_eq!(vertical_extent("あいう\nかきくけこ", 20.0, &typo(), None), (50.0, 100.0));
    }
}
