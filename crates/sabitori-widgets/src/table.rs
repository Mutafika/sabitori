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
use sabitori_core::{Color, ScrollbarStyle, ViewContext};

/// 1 列の定義。 `width` が `None` の列は残り幅を等分する。
///
/// ## 幅が足りないとき ([#96])
///
/// 表は列を**黙って潰さない**。足りなければ次の順で逃がす。
///
/// 1. [`priority`](Self::priority) が 1 以上の列を、数字の大きい方から隠す。
/// 2. それでも足りなければ、見出しと本体を一緒に**横へスクロール**させる。
///
/// 「足りない」は固定列の幅と伸縮列の下限 ([`min`](Self::min)) の合計で
/// 決める。伸縮列の下限は、書かなければ見出しの文字幅 (+ 左右の余白)。
///
/// [#96]: https://github.com/Mutafika/sabitori/issues/96
#[derive(Clone, Debug)]
pub struct TableColumn {
    pub label: String,
    /// 固定幅 (px)。 `None` なら伸縮 (`flex_1`)。
    pub width: Option<f32>,
    /// 伸縮列の下限 (px)。 `None` なら見出しの文字幅 + 左右の余白。
    /// 固定列では使わない (幅がそのまま下限)。
    pub min: Option<f32>,
    /// 幅が足りないときに隠す順。 **0 (既定) は隠さない。** 1 以上は隠してよい
    /// 列で、**数字が大きいほど先に隠れる** (1 がいちばん最後まで残る)。
    pub priority: u8,
}

impl TableColumn {
    /// 伸縮する列。
    pub fn flex(label: impl Into<String>) -> Self {
        Self { label: label.into(), width: None, min: None, priority: 0 }
    }

    /// 固定幅の列。
    pub fn fixed(label: impl Into<String>, width: f32) -> Self {
        Self { label: label.into(), width: Some(width), min: None, priority: 0 }
    }

    /// 伸縮列の下限。 これより狭くなるくらいなら表が横にスクロールする。
    ///
    /// ```ignore
    /// TableColumn::flex("氏名").min(120.0)
    /// ```
    pub fn min(mut self, px: f32) -> Self {
        self.min = Some(px.max(0.0));
        self
    }

    /// 幅が足りないときに隠してよい列にする。 **数字が大きいほど先に隠れる。**
    ///
    /// 業務の一覧は「狭い窓では番号・名前・状態だけ見えればいい」ことが多い。
    /// 残したい列は 0 (既定) のまま、補足の列に 1, 2, .. を付ける。
    ///
    /// ```ignore
    /// TableColumn::fixed("ランク", 110.0).priority(2),   // 最初に隠れる
    /// TableColumn::fixed("累計額", 120.0).priority(1),   // 次に隠れる
    /// ```
    pub fn priority(mut self, n: u8) -> Self {
        self.priority = n;
        self
    }
}

/// 列の並べ方 — どの列を出し、行の最小幅をいくつにするか。
#[derive(Clone, Debug, PartialEq)]
struct ColumnPlan {
    /// 出す列 (元の添字、左から順)。
    visible: Vec<usize>,
    /// 列ごとの下限 (元の添字で引く)。
    mins: Vec<f32>,
    /// 出す列の下限の合計。 行はこれより狭くならない (超えたら横スクロール)。
    need: f32,
}

/// 列ごとの下限 `mins` と、使える幅 `avail` から [`ColumnPlan`] を決める。
///
/// `avail` が `None` (初回フレームでまだ測れていない) なら全部出す。
fn plan_columns(columns: &[TableColumn], mins: &[f32], avail: Option<f32>) -> ColumnPlan {
    let mut visible: Vec<usize> = (0..columns.len()).collect();
    let need_of = |v: &[usize]| v.iter().map(|&i| mins[i]).sum::<f32>();
    if let Some(avail) = avail {
        while need_of(&visible) > avail + 0.5 {
            // 数字の大きい列から。 同じなら右の列から。
            let drop = visible
                .iter()
                .enumerate()
                .filter(|&(_, &i)| columns[i].priority > 0)
                .max_by_key(|&(pos, &i)| (columns[i].priority, pos))
                .map(|(pos, _)| pos);
            match drop {
                Some(pos) => {
                    visible.remove(pos);
                }
                None => break,
            }
        }
    }
    let need = need_of(&visible);
    ColumnPlan { visible, mins: mins.to_vec(), need }
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
    /// 本体のスクロール帯 ([#90])。`None` なら出さない。
    ///
    /// 本体の `.scroll` は表の内側にあってアプリから `.scrollbar(..)` を
    /// 繋げないので、ここで渡す。`from_theme` / `default_dark` は掴める帯を入れる。
    ///
    /// [#90]: https://github.com/Mutafika/sabitori/issues/90
    pub scrollbar: Option<ScrollbarStyle>,
}

impl TableStyle {
    /// [`AppTheme`] から組む。
    ///
    /// **色はテーマから、寸法は `default_dark()` と同じ**。テーマを差し替えても
    /// ウィジェットが追従しないのが [#65] の中身で、既定が全部「いつもの紫」に
    /// なっていた。`ctx.theme` をそのまま渡せる:
    ///
    /// ```ignore
    /// table(ctx, "id", &state, &TableStyle::from_theme(&ctx.theme))
    /// ```
    ///
    /// [`AppTheme`]: sabitori_core::AppTheme
    /// [#65]: https://github.com/Mutafika/sabitori/issues/65
    pub fn from_theme(theme: &sabitori_core::AppTheme) -> Self {
        Self {
            header_bg: theme.elevated,
            header_fg: theme.text_secondary,
            row_bg: theme.surface,
            row_bg_alt: theme.bg,
            row_bg_hover: theme.hover_bg,
            row_bg_selected: theme.select_bg,
            fg: theme.text_primary,
            fg_selected: theme.text_primary,
            border: theme.border,
            scrollbar: Some(ScrollbarStyle::from_theme(theme)),
            ..Self::default_dark()
        }
    }

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
            scrollbar: Some(ScrollbarStyle::default_dark()),
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
/// > **表は自分の箱いっぱいに広がる前提。箱に大きさを与えること。**
/// >
/// > 伸縮列 ([`TableColumn::flex`]) は親の幅から余りを取り、本体は
/// > `flex_1` で残りの高さを取る。そのため**大きさが中身なりの入れ物**に
/// > 入れると、幅ゼロの列と高さゼロの本体になって**中身ごと消える**
/// > (panic もログも無い)。**根に置くだけでは足りない** — 根も中身なりの
/// > 大きさになる。`table(..).w_full().h_full()` と書くか、大きさのある
/// > 入れ物の中で `.flex_1()` を足すこと。
pub fn table(ctx: &ViewContext, id: &str, state: &TableState, style: &TableStyle) -> Element {
    table_with(ctx, id, state, style, |_, _, _| None)
}

/// [`table`] に**セルの中身を自分で組む口**を足した版 ([#75] の 3)。
///
/// `render_cell(row, col, cell)` が `Some(element)` を返したセルは、その要素を
/// 描く。`None` なら今までどおり [`Cell`] の文字を描く。ステータスのバッジ、
/// 行内のボタン、進捗バーなど「文字では足りないセル」のための口 — これが
/// 無くて、色付きの文字で代用していた。
///
/// ```ignore
/// table_with(ctx, "vehicles", &self.table, &style, |row, col, cell| {
///     (col == 2).then(|| badge(&cell.text, self.status_color(row)))
/// })
/// ```
///
/// [`Cell::text`] は `Some` を返した場合も**読み上げに使われる**ので、
/// バッジにも文字の等価物を入れておくこと。
///
/// > **セルの中に `.id()` を置くなら、行ごとに違う id にすること。** 同じ id が
/// > 木に 2 つあると、片方のアニメーション状態がもう片方に漏れる。
///
/// [#75]: https://github.com/Mutafika/sabitori/issues/75
pub fn table_with(
    ctx: &ViewContext,
    id: &str,
    state: &TableState,
    style: &TableStyle,
    render_cell: impl Fn(usize, usize, &Cell) -> Option<Element>,
) -> Element {
    let body_id = format!("{id}::body");
    let gutter = style.scrollbar.as_ref().map_or(0.0, bar_gutter);

    // どの列を出し、行を何 px より狭くしないか (#96)。使える幅は前のフレームで
    // 測った本体の幅から帯の溝を引いたもの。表の幅は親が決め、列の出し方には
    // 依らないので、隠したり流したりしても次のフレームで揺れ戻らない。
    let mins: Vec<f32> = state
        .columns
        .iter()
        .map(|c| match c.width {
            Some(w) => w,
            None => c.min.unwrap_or_else(|| {
                (ctx.text_width(&c.label, style.font_size, false) + 2.0 * style.cell_padding_x)
                    .ceil()
            }),
        })
        .collect();
    let avail = ctx
        .scroll_info(&body_id)
        .map(|i| (i.viewport_width - gutter).max(0.0));
    let plan = plan_columns(&state.columns, &mins, avail);
    let scroll_x = ctx.scroll_info(&body_id).map_or(0.0, |i| i.scroll_x);

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
        body_children.push(table_row(ctx, id, state, style, &plan, row, &render_cell));
    }
    if spacer_bottom > 0.0 {
        body_children.push(div().h(Px(spacer_bottom)).shrink(0.0));
    }

    let mut body = div()
        .id(&body_id)
        .scroll(&body_id)
        .flex_1()
        .flex_col()
        .children(body_children);
    if let Some(bar) = &style.scrollbar {
        body = body.pr(Px(gutter)).scrollbar_style(bar);
    }

    // 見出しの下の区切り線。 `border()` は 4 辺に付いてしまうので 1px の div。
    let rule = div().w_full().h(Px(1.0)).shrink(0.0).bg(style.border);

    div()
        .id(id)
        .role(Role::Table)
        .flex_col()
        .children([header(id, state, style, &plan, gutter, scroll_x), rule, body])
}

/// 帯のために本体の右へ空ける溝。見出しにも同じだけ空けて列を揃える。
///
/// 掴める帯は右端 `lane` px の押しを食うので、行をそこまで伸ばすと行末の
/// セル (`table_with` のボタンなど) の右側が押せなくなる。掴めない帯なら
/// 描かれる所 (右端から `BAR_INSET`) だけ空ける。
fn bar_gutter(bar: &ScrollbarStyle) -> f32 {
    bar.grab.unwrap_or(sabitori_core::scrollbar::BAR_INSET)
}

/// 見出し。 本体が横に流れたら**同じだけ**ずらす (#96)。
///
/// 見出しは縦には流れないので本体の中には置けない。本体と同じ幅・同じ溝の
/// 枠を `scroll_manual` で作り、本体の `scroll_x` をそのまま渡す。本体の位置は
/// `view()` の直前にランタイムから貰った値で、同じフレームの本体にも同じ値が
/// 当たるので、1 フレームも遅れない。
fn header(
    id: &str,
    state: &TableState,
    style: &TableStyle,
    plan: &ColumnPlan,
    gutter: f32,
    scroll_x: f32,
) -> Element {
    let cells: Vec<Element> = plan
        .visible
        .iter()
        .map(|&col| {
            let c = &state.columns[col];
            let label = text(c.label.clone())
                .font_size(style.font_size)
                .color(style.header_fg);
            sized(div(), c, plan.mins[col])
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

    let row = div()
        .role(Role::Row)
        .w_full()
        .min_w(Px(plan.need))
        .h(Px(style.row_height))
        .shrink(0.0)
        .flex_row()
        .children(cells);
    div()
        .w_full()
        .h(Px(style.row_height))
        .shrink(0.0)
        .pr(Px(gutter))
        .bg(style.header_bg)
        .scroll_manual(scroll_x, 0.0)
        .flex_col()
        .child(row)
}

fn table_row(
    ctx: &ViewContext,
    id: &str,
    state: &TableState,
    style: &TableStyle,
    plan: &ColumnPlan,
    row: usize,
    render_cell: &impl Fn(usize, usize, &Cell) -> Option<Element>,
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

    let cells: Vec<Element> = plan
        .visible
        .iter()
        .map(|&col| {
            let c = &state.columns[col];
            let cell = state.rows.get(row).and_then(|r| r.get(col));
            // アプリが組んだ中身が先。無ければ今までどおり文字を描く。
            let custom = cell.and_then(|cell| render_cell(row, col, cell));
            let content = cell.map(|c| c.text.as_str()).unwrap_or("");
            let mut label = text(content)
                .font_size(style.font_size)
                .color(cell.and_then(|c| c.color).unwrap_or(fg));
            if cell.is_some_and(|c| c.bold) {
                label = label.bold();
            }
            let inner = custom.unwrap_or(label);
            sized(div(), c, plan.mins[col])
                .role(Role::Cell)
                .h_full()
                .px_pad(Px(style.cell_padding_x))
                .flex_row()
                .items_center()
                .overflow_hidden()
                .child(inner)
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
        .min_w(Px(plan.need))
        .h(Px(style.row_height))
        .shrink(0.0)
        .bg(bg)
        .flex_row()
        .children(cells)
}

/// 固定幅なら `.w()`、 伸縮なら `.flex_1()` と下限。 列定義の唯一の分岐。
///
/// 伸縮列に下限を付けないと、固定列の合計が表より広いときに 0 幅まで潰れて
/// **列ごと消える** (セルは `overflow_hidden` なので文字も黙って切れる) — #96。
/// `min` は [`plan_columns`] に渡したのと同じ値 ([`TableColumn::min`]、無ければ
/// 見出しの文字幅)。行の最小幅だけで押さえると伸縮列同士で等分され、下限の
/// 大きい列がそれを割るので、セルごとにも付ける。
fn sized(el: Element, c: &TableColumn, min: f32) -> Element {
    match c.width {
        Some(w) => el.w(Px(w)).shrink(0.0),
        None => el.flex_1().min_w(Px(min)),
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

    fn cols(spec: &[(f32, u8)]) -> (Vec<TableColumn>, Vec<f32>) {
        let cols = spec
            .iter()
            .map(|&(w, p)| TableColumn::fixed("c", w).priority(p))
            .collect();
        (cols, spec.iter().map(|&(w, _)| w).collect())
    }

    /// 測れていない (初回) なら全部出す。
    #[test]
    fn an_unmeasured_table_shows_everything() {
        let (c, m) = cols(&[(100.0, 0), (100.0, 1)]);
        let p = plan_columns(&c, &m, None);
        assert_eq!(p.visible, vec![0, 1]);
        assert_eq!(p.need, 200.0);
    }

    /// 数字の大きい列から隠し、足りたら止める。0 は隠さない。
    #[test]
    fn columns_hide_in_priority_order_and_stop_when_they_fit() {
        let (c, m) = cols(&[(100.0, 0), (100.0, 1), (100.0, 3), (100.0, 2)]);
        assert_eq!(plan_columns(&c, &m, Some(400.0)).visible, vec![0, 1, 2, 3]);
        assert_eq!(plan_columns(&c, &m, Some(350.0)).visible, vec![0, 1, 3], "3 が先");
        assert_eq!(plan_columns(&c, &m, Some(250.0)).visible, vec![0, 1], "次に 2");
        let p = plan_columns(&c, &m, Some(50.0));
        assert_eq!(p.visible, vec![0], "0 は幅が足りなくても残る");
        assert_eq!(p.need, 100.0, "残りは横スクロールで逃がす");
    }

    /// 同じ優先度なら右の列から隠す。
    #[test]
    fn ties_hide_from_the_right() {
        let (c, m) = cols(&[(100.0, 1), (100.0, 1), (100.0, 1)]);
        assert_eq!(plan_columns(&c, &m, Some(250.0)).visible, vec![0, 1]);
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
