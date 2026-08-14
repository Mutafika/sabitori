//! 宣言的な表。 `view()` から呼んで [`Element`] を受け取る。
//!
//! ```ignore
//! // view():
//! table(ctx, "files", &self.table, &TableStyle::default_dark())
//!
//! // on_click():
//! if let Some(row) = table_clicked_row("files", id) { self.table.selected = Some(row); }
//! ```
//!
//! ## 0.4.0 での作り直し
//!
//! 旧 `Table` は `new(bounds: Rect, ..)` に画面座標を渡し、 `column_xs()` /
//! `header_cell_rect()` で自分でセルの矩形を計算し、 `on_click(point: Point)`
//! で自分で当たり判定をする retained 型だった。 `view()` からは使えず、
//! repo 内の使用箇所も 0 だった。
//!
//! 宣言版では列幅は taffy が、 当たり判定は id が、 スクロールは
//! `.scroll(id)` が持つ。 だから widget 側に幾何演算は 1 行も要らない。
//! **行の仮想化も `ctx.visible_range()` 任せ**で、 10 万行でも見えている行しか
//! Element を作らない。

use sabitori_core::element::{div, text, Element, Px, Role};
use sabitori_core::{Color, ViewContext};

/// 1 列の定義。 `width` が `None` の列は残り幅を等分する。
#[derive(Clone, Debug)]
pub struct TableColumn {
    pub label: String,
    /// 固定幅 (px)。 `None` なら伸縮 (`flex_1`)。
    pub width: Option<f32>,
}

impl TableColumn {
    /// 伸縮する列。
    pub fn flex(label: impl Into<String>) -> Self {
        Self { label: label.into(), width: None }
    }

    /// 固定幅の列。
    pub fn fixed(label: impl Into<String>, width: f32) -> Self {
        Self { label: label.into(), width: Some(width) }
    }
}

/// セル 1 つ。 色を上書きしたいときだけ `colored` を使う。
#[derive(Clone, Debug)]
pub struct Cell {
    pub text: String,
    pub color: Option<Color>,
    pub bold: bool,
}

impl Cell {
    pub fn text(s: impl Into<String>) -> Self {
        Self { text: s.into(), color: None, bold: false }
    }

    pub fn colored(s: impl Into<String>, color: Color) -> Self {
        Self { text: s.into(), color: Some(color), bold: false }
    }

    pub fn bold(s: impl Into<String>) -> Self {
        Self { text: s.into(), color: None, bold: true }
    }
}

/// 表の状態。 中身と選択だけを持つ。 **スクロール位置は持たない** —
/// それはランタイムが `.scroll(id)` で持つ (issue #14 の所有権の話)。
#[derive(Clone, Debug, Default)]
pub struct TableState {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<Vec<Cell>>,
    /// 選択中の行 (元データの添字)。
    pub selected: Option<usize>,
}

impl TableState {
    pub fn new(columns: Vec<TableColumn>) -> Self {
        Self { columns, rows: Vec::new(), selected: None }
    }

    pub fn set_rows(&mut self, rows: Vec<Vec<Cell>>) {
        self.rows = rows;
        if let Some(sel) = self.selected {
            if sel >= self.rows.len() {
                self.selected = None;
            }
        }
    }

    /// 選択を 1 つ下へ。 未選択なら先頭。
    pub fn select_next(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(i) if i + 1 < self.rows.len() => i + 1,
            Some(i) => i,
            None => 0,
        });
    }

    /// 選択を 1 つ上へ。 未選択なら先頭。
    pub fn select_prev(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(i) => i.saturating_sub(1),
            None => 0,
        });
    }
}

/// 表の見た目。
#[derive(Clone, Debug)]
pub struct TableStyle {
    pub header_bg: Color,
    pub header_fg: Color,
    pub row_bg: Color,
    pub row_bg_alt: Color,
    pub row_bg_hover: Color,
    pub row_bg_selected: Color,
    pub fg: Color,
    pub fg_selected: Color,
    pub border: Color,
    pub row_height: f32,
    pub font_size: f32,
    pub cell_padding_x: f32,
}

impl TableStyle {
    pub fn default_dark() -> Self {
        Self {
            header_bg: Color::from_hex("#1a1a2e"),
            header_fg: Color::from_hex("#8a8aa8"),
            row_bg: Color::TRANSPARENT,
            row_bg_alt: Color::new(1.0, 1.0, 1.0, 0.02),
            row_bg_hover: Color::from_hex("#24243a"),
            row_bg_selected: Color::from_hex("#2a3a6a"),
            fg: Color::from_hex("#c8c8dc"),
            fg_selected: Color::from_hex("#ffffff"),
            border: Color::from_hex("#2a2a44"),
            row_height: 28.0,
            font_size: 13.0,
            cell_padding_x: 10.0,
        }
    }
}

/// 行 `row` の要素 id。 `on_click` で突き合わせる。
pub fn table_row_id(id: &str, row: usize) -> String {
    format!("{id}::row:{row}")
}

/// 列見出し `col` の要素 id。 ソートの切り替えに使う。
pub fn table_header_id(id: &str, col: usize) -> String {
    format!("{id}::col:{col}")
}

/// クリックされた id が表の行なら、 その行番号。
///
/// ```ignore
/// fn on_click(&mut self, id: &str) {
///     if let Some(row) = table_clicked_row("files", id) {
///         self.table.selected = Some(row);
///     }
/// }
/// ```
pub fn table_clicked_row(id: &str, clicked: &str) -> Option<usize> {
    clicked.strip_prefix(&format!("{id}::row:"))?.parse().ok()
}

/// クリックされた id が列見出しなら、 その列番号。
pub fn table_clicked_header(id: &str, clicked: &str) -> Option<usize> {
    clicked.strip_prefix(&format!("{id}::col:"))?.parse().ok()
}

/// 表を組み立てる。
///
/// `id` はスクロールコンテナの id でもある。 高さは呼び出し側が決める
/// (`.h(Px(..))` か `.flex_1()` を結果に繋ぐ)。
pub fn table(ctx: &ViewContext, id: &str, state: &TableState, style: &TableStyle) -> Element {
    let body_id = format!("{id}::body");

    // 見えている行だけ作る。 ランタイムが持つスクロール位置から範囲を貰う。
    // 初回フレームはまだ測れていないので、 `visible_range` は広めの既定を返す。
    let (first, count) = ctx.visible_range(&body_id, style.row_height);
    let end = (first + count).min(state.rows.len());
    let first = first.min(end);

    // 上下に「見えていない行ぶんの高さ」を積んで、 スクロール量を実データに合わせる。
    let spacer_top = first as f32 * style.row_height;
    let spacer_bottom = (state.rows.len().saturating_sub(end)) as f32 * style.row_height;

    let mut body_children = Vec::with_capacity(end - first + 2);
    if spacer_top > 0.0 {
        body_children.push(div().h(Px(spacer_top)).shrink(0.0));
    }
    for row in first..end {
        body_children.push(table_row(ctx, id, state, style, row));
    }
    if spacer_bottom > 0.0 {
        body_children.push(div().h(Px(spacer_bottom)).shrink(0.0));
    }

    let body = div()
        .id(&body_id)
        .scroll(&body_id)
        .flex_1()
        .flex_col()
        .children(body_children);

    // 見出しの下の区切り線。 `border()` は 4 辺に付いてしまうので 1px の div。
    let rule = div().w_full().h(Px(1.0)).shrink(0.0).bg(style.border);

    div()
        .id(id)
        .role(Role::Table)
        .flex_col()
        .children([header(id, state, style), rule, body])
}

fn header(id: &str, state: &TableState, style: &TableStyle) -> Element {
    let cells: Vec<Element> = state
        .columns
        .iter()
        .enumerate()
        .map(|(col, c)| {
            let label = text(c.label.clone())
                .font_size(style.font_size)
                .color(style.header_fg);
            sized(div(), c.width)
                .id(&table_header_id(id, col))
                .role(Role::ColumnHeader)
                .label(&c.label)
                .h_full()
                .px_pad(Px(style.cell_padding_x))
                .flex_row()
                .items_center()
                .overflow_hidden()
                .child(label)
        })
        .collect();

    div()
        .role(Role::Row)
        .w_full()
        .h(Px(style.row_height))
        .shrink(0.0)
        .bg(style.header_bg)
        .flex_row()
        .children(cells)
}

fn table_row(
    ctx: &ViewContext,
    id: &str,
    state: &TableState,
    style: &TableStyle,
    row: usize,
) -> Element {
    let row_id = table_row_id(id, row);
    let selected = state.selected == Some(row);
    let hovered = ctx.hovered.as_deref() == Some(row_id.as_str());

    let bg = if selected {
        style.row_bg_selected
    } else if hovered {
        style.row_bg_hover
    } else if row % 2 == 1 {
        style.row_bg_alt
    } else {
        style.row_bg
    };
    let fg = if selected { style.fg_selected } else { style.fg };

    let cells: Vec<Element> = state.columns
        .iter()
        .enumerate()
        .map(|(col, c)| {
            let cell = state.rows.get(row).and_then(|r| r.get(col));
            let content = cell.map(|c| c.text.as_str()).unwrap_or("");
            let mut label = text(content)
                .font_size(style.font_size)
                .color(cell.and_then(|c| c.color).unwrap_or(fg));
            if cell.is_some_and(|c| c.bold) {
                label = label.bold();
            }
            sized(div(), c.width)
                .role(Role::Cell)
                .h_full()
                .px_pad(Px(style.cell_padding_x))
                .flex_row()
                .items_center()
                .overflow_hidden()
                .child(label)
        })
        .collect();

    // 行ラベルは 1 行ぶんの読み上げ内容 — セルを繋いだもの。
    let spoken = state
        .rows
        .get(row)
        .map(|r| r.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();

    div()
        .id(&row_id)
        .role(Role::Row)
        .label(&spoken)
        .w_full()
        .h(Px(style.row_height))
        .shrink(0.0)
        .bg(bg)
        .flex_row()
        .children(cells)
}

/// 固定幅なら `.w()`、 伸縮なら `.flex_1()`。 列定義の唯一の分岐。
fn sized(el: Element, width: Option<f32>) -> Element {
    match width {
        Some(w) => el.w(Px(w)).shrink(0.0),
        None => el.flex_1(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(rows: usize) -> TableState {
        let mut s = TableState::new(vec![
            TableColumn::flex("名前"),
            TableColumn::fixed("サイズ", 80.0),
        ]);
        s.set_rows(
            (0..rows)
                .map(|i| vec![Cell::text(format!("file-{i}")), Cell::text("1 KB")])
                .collect(),
        );
        s
    }

    /// id の往復。 これが壊れると `on_click` が行を特定できなくなる。
    #[test]
    fn row_ids_round_trip() {
        let id = table_row_id("files", 42);
        assert_eq!(table_clicked_row("files", &id), Some(42));
        // 別の表の行は拾わない。
        assert_eq!(table_clicked_row("other", &id), None);
        // 見出しは行ではない。
        assert_eq!(table_clicked_row("files", &table_header_id("files", 1)), None);
        assert_eq!(table_clicked_header("files", &table_header_id("files", 1)), Some(1));
    }

    /// 選択の上下移動が範囲を出ないこと。
    #[test]
    fn selection_stays_in_range() {
        let mut s = state(3);
        s.select_prev();
        assert_eq!(s.selected, Some(0), "未選択からは先頭");
        s.select_prev();
        assert_eq!(s.selected, Some(0), "先頭より上には行かない");
        for _ in 0..10 {
            s.select_next();
        }
        assert_eq!(s.selected, Some(2), "末尾より下には行かない");
    }

    /// 行が減ったら、 範囲外を指したままの選択は捨てること。
    #[test]
    fn shrinking_the_rows_drops_a_stale_selection() {
        let mut s = state(5);
        s.selected = Some(4);
        s.set_rows(vec![vec![Cell::text("only")]]);
        assert_eq!(s.selected, None);
    }
}
