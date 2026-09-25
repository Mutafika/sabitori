//! **等幅の文字の格子** — 端末・ログビューア・16 進ダンプ・等幅の表
//! ([#102](https://github.com/Mutafika/sabitori/issues/102))。
//!
//! `text()` を並べると、要素数・シェーピングのキャッシュ・格子との字送りの
//! どれかで必ず損をする:
//!
//! - 1 セル 1 要素なら、80×24 で要素が千を超え、taffy の木も毎フレーム作り直す
//! - 1 行 1 要素 (`color_spans`) なら、行の文字列が 1 字変わるだけで別の鍵になり、
//!   シェーピングのキャッシュに当たらない (mearie の実測で CPU が 7 倍)
//! - シェープした字送りはセル幅と少しずつずれ、行末ほど背景・カーソルから離れる
//!
//! 格子はそもそもシェーピングが要らない。1 セルに 1 字形を置くだけなので、
//! **字形は文字単位でキャッシュ**でき、位置は `col * cell_w` で決まる。
//! [`cell_grid`](crate::element::cell_grid) はレイアウト上ただの箱 1 つで、
//!
//! - 背景は、行ごとに同じ色の続きをまとめて矩形 1 つにする
//! - 字形は `(col * cell_w, row * cell_h)` に置く (字送りのズレは起きない)
//! - 中身が前のフレームと同じ行は、前の字形を使い回す (要素に `id` があるとき)
//! - 描くのは見えている行だけ (スクロールの中の長いログでも重くならない)
//!
//! 選択・ヒットテスト・カーソル・IME の変換中の文字は持たない。要素の矩形は
//! 分かるので、呼ぶ側が格子の上に普通の要素として重ねる。合字は使わない
//! (使わないことが、文字単位でキャッシュできる前提)。

use crate::Color;

/// セルの飾り。ビットの組み合わせ。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellFlags(pub u8);

impl CellFlags {
    pub const NONE: CellFlags = CellFlags(0);
    pub const BOLD: CellFlags = CellFlags(1 << 0);
    pub const ITALIC: CellFlags = CellFlags(1 << 1);
    pub const UNDERLINE: CellFlags = CellFlags(1 << 2);
    pub const STRIKE: CellFlags = CellFlags(1 << 3);
    /// 2 セル幅の字 (CJK など)。字形はこのセルから右へ 2 セル分に置く。
    pub const WIDE: CellFlags = CellFlags(1 << 4);
    /// 2 セル幅の字の右半分。字形は描かない (背景・下線は描く)。
    pub const WIDE_SPACER: CellFlags = CellFlags(1 << 5);
    /// 薄く (前景色の不透明度を半分に)。
    pub const DIM: CellFlags = CellFlags(1 << 6);

    pub const fn contains(self, other: CellFlags) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for CellFlags {
    type Output = CellFlags;
    fn bitor(self, rhs: CellFlags) -> CellFlags {
        CellFlags(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for CellFlags {
    fn bitor_assign(&mut self, rhs: CellFlags) {
        self.0 |= rhs.0;
    }
}

/// 1 セル。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridCell {
    /// 描く字。空白 (`' '`) と `'\0'` は字形を描かない。
    pub ch: char,
    pub fg: Color,
    /// `None` = 塗らない (要素の背景が見える)。
    pub bg: Option<Color>,
    pub flags: CellFlags,
}

impl GridCell {
    /// 空白のセル。
    pub const fn blank(fg: Color) -> Self {
        GridCell { ch: ' ', fg, bg: None, flags: CellFlags::NONE }
    }
}

impl Default for GridCell {
    fn default() -> Self {
        GridCell::blank(Color::WHITE)
    }
}

/// 格子の中身。`Arc<CellGrid>` にして [`cell_grid`](crate::element::cell_grid) に渡す。
#[derive(Clone, Debug, PartialEq)]
pub struct CellGrid {
    pub cols: usize,
    pub rows: usize,
    /// `rows * cols` 個、行ごとに左から。
    ///
    /// 直接書き換えてよい。字形の使い回しは行の**中身を比べて**決めるので、
    /// 「変えたら知らせる」約束は無い。
    pub cells: Vec<GridCell>,
}

impl CellGrid {
    /// 空白で埋めた格子。
    pub fn new(cols: usize, rows: usize, fg: Color) -> Self {
        Self {
            cols,
            rows,
            cells: vec![GridCell::blank(fg); cols * rows],
        }
    }

    pub fn get(&self, col: usize, row: usize) -> Option<&GridCell> {
        if col < self.cols && row < self.rows {
            self.cells.get(row * self.cols + col)
        } else {
            None
        }
    }

    /// セルを書き換える。範囲の外は捨てる。
    pub fn set(&mut self, col: usize, row: usize, cell: GridCell) {
        if col < self.cols && row < self.rows {
            self.cells[row * self.cols + col] = cell;
        }
    }

    /// 1 行ぶんのセル。
    pub fn row(&self, row: usize) -> &[GridCell] {
        let start = (row * self.cols).min(self.cells.len());
        let end = (start + self.cols).min(self.cells.len());
        &self.cells[start..end]
    }

    /// 文字列を `(col, row)` から書く (はみ出した分は捨てる)。
    pub fn put_str(&mut self, col: usize, row: usize, s: &str, fg: Color, bg: Option<Color>, flags: CellFlags) {
        for (i, ch) in s.chars().enumerate() {
            self.set(col + i, row, GridCell { ch, fg, bg, flags });
        }
    }

    /// 行の背景を、同じ色の続きごとの `(開始列, 列数, 色)` にまとめる。
    pub fn bg_runs(&self, row: usize) -> Vec<(usize, usize, Color)> {
        let mut out: Vec<(usize, usize, Color)> = Vec::new();
        for (col, cell) in self.row(row).iter().enumerate() {
            let Some(bg) = cell.bg else { continue };
            match out.last_mut() {
                Some((start, len, color)) if *start + *len == col && *color == bg => *len += 1,
                _ => out.push((col, 1, bg)),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FG: Color = Color::WHITE;
    const RED: Color = Color::new(1.0, 0.0, 0.0, 1.0);
    const BLUE: Color = Color::new(0.0, 0.0, 1.0, 1.0);

    #[test]
    fn neighbouring_cells_of_one_colour_become_one_run() {
        let mut g = CellGrid::new(8, 1, FG);
        g.put_str(1, 0, "abc", FG, Some(RED), CellFlags::NONE);
        g.put_str(4, 0, "de", FG, Some(BLUE), CellFlags::NONE);
        g.put_str(7, 0, "f", FG, Some(RED), CellFlags::NONE);
        assert_eq!(g.bg_runs(0), vec![(1, 3, RED), (4, 2, BLUE), (7, 1, RED)]);
    }

    #[test]
    fn out_of_range_writes_are_dropped() {
        let mut g = CellGrid::new(2, 1, FG);
        g.put_str(1, 0, "xyz", FG, None, CellFlags::NONE);
        assert_eq!(g.row(0)[1].ch, 'x');
        assert_eq!(g.cells.len(), 2);
        g.set(0, 5, GridCell::default());
    }

    #[test]
    fn flags_combine() {
        let f = CellFlags::BOLD | CellFlags::UNDERLINE;
        assert!(f.contains(CellFlags::BOLD));
        assert!(!f.contains(CellFlags::ITALIC));
    }
}
