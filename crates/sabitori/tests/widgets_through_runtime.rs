//! `sabitori-widgets` を **ランタイム越しに**動かすテスト。
//!
//! ## なぜ `sabitori` 側に置くのか
//!
//! 依存の向きが `sabitori → sabitori-widgets` なので、 widget crate からは
//! [`Harness`] に手が届かない。 そのため 0.4.0 より前の widget テストは全部
//! 「関数を直接呼んで戻り値を見る」単体テストで、 **クリックしたら実際に
//! 反応するか**を通す経路が 1 本も無かった。 `table` / `tree_view` /
//! `virtual_list` / `tooltip` / `panel` / `modal` はテスト 0 件だった。
//!
//! ここに置けば消費側とまったく同じ経路 (build → hit region → on_click) を
//! 通せる。 widget が Element を返す形に揃った (0.4.0) からこそ書ける。

use sabitori::testing::Harness;
use sabitori::{div, DeclarativeApp, Element, InputEvent, Px, ScrollIntent, ViewContext};
use sabitori_widgets::{
    split_pane, table, table_clicked_row, tree_clicked_row, tree_view, virtual_list, Cell,
    SplitDirection, SplitPaneState, SplitPaneStyle, TableColumn, TableState, TableStyle, TreeNode,
    TreeViewStyle,
};

// ---------------------------------------------------------------------------
// table
// ---------------------------------------------------------------------------

struct TableApp {
    state: TableState,
    clicked_header: Option<usize>,
}

impl TableApp {
    fn new(rows: usize) -> Self {
        let mut state = TableState::new(vec![
            TableColumn::flex("名前"),
            TableColumn::fixed("サイズ", 80.0),
        ]);
        state.set_rows(
            (0..rows)
                .map(|i| vec![Cell::text(format!("file-{i}")), Cell::text("1 KB")])
                .collect(),
        );
        Self { state, clicked_header: None }
    }
}

impl sabitori::DeclarativeApp for TableApp {
    fn view(&self, ctx: &ViewContext) -> Element {
        table(ctx, "files", &self.state, &TableStyle::default_dark())
            .w_full()
            .h_full()
    }

    fn on_click(&mut self, id: &str) {
        if let Some(row) = table_clicked_row("files", id) {
            self.state.selected = Some(row);
        }
        if let Some(col) = sabitori_widgets::table_clicked_header("files", id) {
            self.clicked_header = Some(col);
        }
    }
}

/// 行をクリックしたら、 その行が選択されること。
///
/// 旧 retained `Table` はこれが `on_click(point: Point) -> TableEvent` で、
/// アプリが座標を自分で渡す必要があった。 宣言版は id 経由なので、
/// ランタイムのクリック配信にそのまま乗る。
#[test]
fn clicking_a_table_row_selects_it() {
    let mut h = Harness::new(TableApp::new(10), 500.0, 400.0);
    h.frame();

    h.click("files::row:3");

    assert_eq!(h.app().state.selected, Some(3));
}

/// 列見出しもクリックできること (ソートの切り替えに使う)。
#[test]
fn clicking_a_column_header_reports_its_index() {
    let mut h = Harness::new(TableApp::new(3), 500.0, 400.0);
    h.frame();

    h.click("files::col:1");

    assert_eq!(h.app().clicked_header, Some(1));
}

/// **表が実際にスクロールすること。** 行が画面外に出たら hit_regions から消える。
///
/// `table` は本体に `.scroll()` を付けているので、 アプリ側の配線は要らない。
#[test]
fn the_table_body_scrolls_and_hides_rows_that_leave_the_viewport() {
    let mut h = Harness::new(TableApp::new(200), 500.0, 300.0);
    h.frame();

    assert!(
        h.visible_ids().iter().any(|id| id == "files::row:0"),
        "最初は先頭行が見えている"
    );

    h.scroll("files::body", 900.0);
    h.frame();

    assert!(
        !h.visible_ids().iter().any(|id| id == "files::row:0"),
        "スクロール後、 先頭行は画面外"
    );
    assert!(
        h.scroll_y("files::body").unwrap_or(0.0) > 0.0,
        "スクロール位置が進んでいること"
    );
}

/// 仮想化していても、 スクロール後に見えている行はクリックできること。
///
/// spacer の高さがずれていると、 押した行と選択される行が食い違う。
#[test]
fn rows_stay_clickable_after_scrolling() {
    let mut h = Harness::new(TableApp::new(200), 500.0, 300.0);
    h.frame();
    h.scroll("files::body", 900.0);
    h.frame();

    // いま見えている行のうち、 表の行であるものを 1 つ選んで押す。
    let visible_row = h
        .visible_ids()
        .into_iter()
        .find_map(|id| table_clicked_row("files", &id))
        .expect("スクロール後も行が見えていること");

    h.click(&sabitori_widgets::table_row_id("files", visible_row));

    assert_eq!(
        h.app().state.selected,
        Some(visible_row),
        "押した行がそのまま選択されること (spacer のズレが無い)"
    );
}

/// 行が 0 でも組み立てが落ちないこと。 空状態は必ず通る道。
#[test]
fn an_empty_table_builds() {
    let mut h = Harness::new(TableApp::new(0), 500.0, 400.0);
    h.frame();
    assert!(h.rect_of("files").is_some(), "表そのものは存在する");
}

// ---------------------------------------------------------------------------
// tree_view
// ---------------------------------------------------------------------------

struct TreeApp {
    root: TreeNode,
}

impl sabitori::DeclarativeApp for TreeApp {
    fn view(&self, ctx: &ViewContext) -> Element {
        tree_view(ctx, "tree", &self.root, &TreeViewStyle::default_dark())
            .w_full()
            .h_full()
    }

    fn on_click(&mut self, id: &str) {
        if let Some(row) = tree_clicked_row("tree", id) {
            self.root.toggle_row(row);
            self.root.select_row(row);
        }
    }
}

/// 木の行をクリックすると開き、 **開いた子が次のフレームで現れること**。
#[test]
fn clicking_a_tree_row_expands_it_and_the_children_appear() {
    let root = TreeNode::new("root")
        .with_children(vec![
            TreeNode::new("dir").with_children(vec![TreeNode::new("child")]),
            TreeNode::new("sibling"),
        ])
        .with_expanded(true);
    let mut h = Harness::new(TreeApp { root }, 400.0, 400.0);
    h.frame();

    // 行: 0=root, 1=dir, 2=sibling
    assert_eq!(h.visible_ids().iter().filter(|id| id.starts_with("tree::row:")).count(), 3);

    h.click("tree::row:1");
    h.frame();

    // 開いたので 4 行 (root, dir, child, sibling)。
    assert_eq!(
        h.visible_ids().iter().filter(|id| id.starts_with("tree::row:")).count(),
        4,
        "子が現れること"
    );
    assert!(h.app().root.children[0].expanded);
}

// ---------------------------------------------------------------------------
// virtual_list
// ---------------------------------------------------------------------------

struct ListApp {
    items: Vec<String>,
}

impl sabitori::DeclarativeApp for ListApp {
    fn view(&self, ctx: &ViewContext) -> Element {
        virtual_list(ctx, "log", &self.items, 20.0, |line, i| {
            div()
                .id(format!("line-{i}"))
                .w_full()
                .h(Px(20.0))
                .child(sabitori::text(line.clone()))
        })
        .w_full()
        .h_full()
    }
}

/// **1 万行でも、 作られる Element は viewport ぶんだけ**であること。
///
/// 旧実装は viewport をウィンドウ高さ (`ctx.height`) から算出していたので、
/// パネルに入れると必要量とかけ離れた数を作っていた。
#[test]
fn a_virtual_list_only_builds_the_visible_rows() {
    let items: Vec<String> = (0..10_000).map(|i| format!("line {i}")).collect();
    let mut h = Harness::new(ListApp { items }, 400.0, 300.0);
    h.frame();

    let built = h
        .visible_ids()
        .iter()
        .filter(|id| id.starts_with("line-"))
        .count();

    assert!(
        built < 100,
        "viewport 300px / 行 20px なので数十行のはず (実際: {built})"
    );
    assert!(built > 0, "1 行も作られないのはおかしい");
}

/// スクロールすると、 表示される行が実際に入れ替わること。
#[test]
fn scrolling_a_virtual_list_swaps_which_rows_exist() {
    let items: Vec<String> = (0..10_000).map(|i| format!("line {i}")).collect();
    let mut h = Harness::new(ListApp { items }, 400.0, 300.0);
    h.frame();
    assert!(h.visible_ids().iter().any(|id| id == "line-0"));

    h.scroll("log", 4000.0);
    h.frame();

    assert!(
        !h.visible_ids().iter().any(|id| id == "line-0"),
        "先頭行はもう作られていない"
    );
    assert!(
        h.visible_ids().iter().any(|id| id.starts_with("line-")),
        "代わりに別の行が作られている"
    );
}

// ---------------------------------------------------------------------------
// split_pane
// ---------------------------------------------------------------------------

struct SplitApp {
    split: SplitPaneState,
    left_clicks: u32,
}

impl sabitori::DeclarativeApp for SplitApp {
    fn view(&self, ctx: &ViewContext) -> Element {
        split_pane(
            ctx,
            "sp",
            &self.split,
            &SplitPaneStyle::default_dark(),
            div().id("left").w_full().h_full(),
            div().id("right").w_full().h_full(),
        )
        .w_full()
        .h_full()
    }

    fn on_click(&mut self, id: &str) {
        if id == "left" {
            self.left_clicks += 1;
        }
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        // 幅は view と同じ 800。 実アプリでは `ctx.width` を持ち回す。
        self.split.on_input("sp", event, None, 800.0)
    }
}

/// 比率どおりの幅で 2 枚が並ぶこと。 flex_grow に載せているので taffy が割る。
#[test]
fn the_two_panes_are_laid_out_by_ratio() {
    let app = SplitApp {
        split: SplitPaneState::new(SplitDirection::Horizontal, 0.25),
        left_clicks: 0,
    };
    let mut h = Harness::new(app, 800.0, 400.0);
    h.frame();

    let left = h.rect_of("left").expect("左ペインがある");
    let right = h.rect_of("right").expect("右ペインがある");

    // 仕切り 6px を除いた 794px を 1:3 で割る。
    assert!(
        (left.size.width - 198.5).abs() < 2.0,
        "左は約 1/4 (実際: {})",
        left.size.width
    );
    assert!(
        right.size.width > left.size.width * 2.5,
        "右のほうがずっと広い"
    );
}

/// **仕切りの上に居ないときの press を食わないこと。**
///
/// ドラッグ判定が雑だと、 ペインの中身のクリックが全部死ぬ。 これは
/// 「widget を置いたらアプリが反応しなくなった」の典型的な原因。
#[test]
fn the_divider_does_not_swallow_clicks_meant_for_the_panes() {
    let app = SplitApp {
        split: SplitPaneState::new(SplitDirection::Horizontal, 0.5),
        left_clicks: 0,
    };
    let mut h = Harness::new(app, 800.0, 400.0);
    h.frame();

    h.click("left");

    assert_eq!(h.app().left_clicks, 1, "左ペインのクリックが届くこと");
    assert!(!h.app().split.is_dragging(), "ドラッグ状態になっていないこと");
}

// ---------------------------------------------------------------------------
// `examples/filer.rs` が使っている形 (spacer + visible_range + scroll_intents)
// ---------------------------------------------------------------------------

/// filer と同じ組み方をした最小のリスト。 0.4.0 で手動 `ScrollView` から
/// ランタイム管理へ移した形が、 実際に成立していることを確かめる。
struct FilerShape {
    rows: usize,
    pending_scroll: Option<f32>,
}

const ROW_H: f32 = 32.0;

impl sabitori::DeclarativeApp for FilerShape {
    fn view(&self, ctx: &ViewContext) -> Element {
        let (first, count) = ctx.visible_range("file-list", ROW_H);
        let last = (first + count).min(self.rows);
        let first = first.min(last);

        let mut children: Vec<Element> = Vec::new();
        if first > 0 {
            children.push(div().h(Px(first as f32 * ROW_H)).shrink(0.0));
        }
        for i in first..last {
            children.push(div().id(format!("f-{i}")).w_full().h(Px(ROW_H)).shrink(0.0));
        }
        let tail = self.rows.saturating_sub(last);
        if tail > 0 {
            children.push(div().h(Px(tail as f32 * ROW_H)).shrink(0.0));
        }

        div()
            .scroll("file-list")
            .w_full()
            .h_full()
            .flex_col()
            .children(children)
    }

    fn scroll_intents(&mut self) -> Vec<ScrollIntent> {
        self.pending_scroll
            .take()
            .map(|y| ScrollIntent::y("file-list", y))
            .into_iter()
            .collect()
    }
}

/// 仮想化していても、 **スクロール可能な総量が実データぶんある**こと。
///
/// spacer を積み忘れると、 中身の高さが「見えている数行ぶん」しか無いことに
/// なり、 少し回しただけで最下部に着く。 filer の旧実装は `mt(top_offset)` で
/// これを避けていたが、 その方式はランタイム管理と両立しない。
#[test]
fn the_spacers_give_the_scroller_the_full_content_height() {
    let mut h = Harness::new(FilerShape { rows: 1000, pending_scroll: None }, 500.0, 400.0);
    h.frame();

    // 深いところまでスクロールできること。
    h.scroll("file-list", 20_000.0);
    h.frame();

    let y = h.scroll_y("file-list").unwrap_or(0.0);
    assert!(
        y > 15_000.0,
        "1000 行 x 32px = 32000px 相当までスクロールできるはず (実際: {y})"
    );
    assert!(
        h.visible_ids().iter().any(|id| id.starts_with("f-")),
        "その位置にも行が作られていること"
    );
}

/// `scroll_intents` からのプログラム的スクロールが効くこと。
///
/// filer の「ディレクトリを移動したら先頭へ戻す」「キーボード選択を画面内に
/// 入れる」がこの口を通る。
#[test]
fn scroll_intents_move_a_runtime_owned_container() {
    let mut h = Harness::new(FilerShape { rows: 1000, pending_scroll: None }, 500.0, 400.0);
    h.frame();
    h.scroll("file-list", 5_000.0);
    h.frame();
    assert!(h.scroll_y("file-list").unwrap_or(0.0) > 0.0);

    // 先頭へ戻す要求を出す。 intent は `smooth_scroll_to` (ばねの目標) なので、
    // frame() だけでは動かない — 時間を進める必要がある。
    h.app_mut().pending_scroll = Some(0.0);
    h.frame();
    h.settle();

    let y = h.scroll_y("file-list").unwrap_or(-1.0);
    assert!(y < 1.0, "intent どおり先頭に戻ること (実際: {y})");
}

// ---------------------------------------------------------------------------
// 配線漏れが「黙って」起きないこと
// ---------------------------------------------------------------------------

use sabitori_widgets::{text_input, TextInputState, TextInputStyle};

fn input_style() -> TextInputStyle {
    TextInputStyle {
        bg: sabitori::Color::from_hex("#202020"),
        border: sabitori::Color::from_hex("#404040"),
        text: sabitori::Color::WHITE,
        placeholder: sabitori::Color::from_hex("#808080"),
        font_size: 14.0,
        radius: 4.0,
        padding: 8.0,
        focus_border: None,
        caret: None,
        preedit: None,
        selection: None,
    }
}

/// **自作の**テキスト欄。 `text_input` を使わず、 自分で `Role::TextInput` を
/// 名乗って focusable にしただけ。 登録していないので配線はアプリの責任。
struct HandRolled {
    typed: String,
    wire_it: bool,
}

impl sabitori::DeclarativeApp for HandRolled {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let mut el = div()
            .id("custom")
            .role(sabitori::Role::TextInput)
            .label("自作の欄")
            .w(Px(200.0))
            .h(Px(32.0));
        el.focusable = true;
        el
    }

    fn on_focused_input(&mut self, id: &str, e: &InputEvent) -> bool {
        if !self.wire_it {
            return false;
        }
        match (id, e) {
            ("custom", InputEvent::CharInput(c)) => {
                self.typed.push(*c);
                true
            }
            _ => false,
        }
    }
}

/// **自作のテキスト欄で配線を忘れたら、 それが観測できること。**
///
/// `text_input` を使う限りこの状況は起きない (置いた時点で登録される) が、
/// 自分で `Role::TextInput` を名乗る欄を作る場合は配線がアプリの責任になる。
/// 忘れると打った文字がどこにも行かない — 黙って落とさないための検出器。
#[test]
fn a_hand_rolled_text_field_with_no_handler_is_reported() {
    let mut h = Harness::new(HandRolled { typed: String::new(), wire_it: false }, 400.0, 200.0);
    h.frame();

    h.click("custom");
    assert_eq!(h.focused_id(), Some("custom"), "フォーカスは入る (だから気づかない)");

    h.text("abc");

    assert_eq!(h.app().typed, "", "文字は入らない");
    assert_eq!(
        h.unrouted_text_inputs(),
        vec!["custom"],
        "そのことが観測できること"
    );
}

/// 配線してあれば何も報告されないこと (誤検知しない)。
#[test]
fn a_wired_hand_rolled_field_reports_nothing() {
    let mut h = Harness::new(HandRolled { typed: String::new(), wire_it: true }, 400.0, 200.0);
    h.frame();

    h.click("custom");
    h.text("abc");

    assert_eq!(h.app().typed, "abc");
    assert!(
        h.unrouted_text_inputs().is_empty(),
        "配線済みなら何も出ない: {:?}",
        h.unrouted_text_inputs()
    );
}

/// `text_input` を使う場合は、 何も実装しなくても報告されないこと。
///
/// 登録済みなのでランタイムが消費する。 詳しくは `tests/zero_wiring.rs`。
#[test]
fn the_builtin_text_input_never_needs_wiring() {
    struct Bare {
        name: TextInputState,
    }
    impl sabitori::DeclarativeApp for Bare {
        fn view(&self, ctx: &ViewContext) -> Element {
            text_input(ctx, "name", &self.name, &input_style())
        }
    }

    let mut h = Harness::new(Bare { name: TextInputState::new("名前") }, 400.0, 200.0);
    h.frame();
    h.click("name");
    h.text("abc");

    assert_eq!(h.app().name.text(), "abc", "配線ゼロで動く");
    assert!(h.unrouted_text_inputs().is_empty());
}

/// フォーカスできるだけの要素 (ボタン等) が打鍵を無視するのは正常なので、
/// そこでは報告しないこと。
#[test]
fn a_focusable_non_text_element_is_not_reported() {
    struct FocusableButton;
    impl sabitori::DeclarativeApp for FocusableButton {
        fn view(&self, _ctx: &ViewContext) -> Element {
            let mut el = div().id("btn").w(Px(80.0)).h(Px(32.0));
            el.focusable = true;
            el
        }
    }
    let mut h = Harness::new(FocusableButton, 400.0, 200.0);
    h.frame();

    h.click("btn");
    h.text("abc");

    assert!(
        h.unrouted_text_inputs().is_empty(),
        "テキスト欄でないものは対象外"
    );
}

// ---------------------------------------------------------------------------
// 横スクロール (#74)
// ---------------------------------------------------------------------------

/// 横にも縦にも動く表 (配車表のガント相当)。
struct Gantt {
    reset_timeline: bool,
    jump_to_row: Option<f32>,
    /// 時間軸の幅。期間・密度を変えると**縮む** (#83)。
    span_px: f32,
}

impl Default for Gantt {
    fn default() -> Self {
        Self { reset_timeline: false, jump_to_row: None, span_px: 3000.0 }
    }
}

impl DeclarativeApp for Gantt {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let span = self.span_px;
        let rows: Vec<Element> = (0..40)
            .map(|_| div().w(Px(span)).h(Px(40.0)))
            .collect();
        div().w_full().h_full().child(
            div()
                .id("timeline")
                .scroll("timeline")
                .w(Px(600.0))
                .h(Px(400.0))
                .flex_col()
                .children(rows),
        )
    }

    fn scroll_intents(&mut self) -> Vec<ScrollIntent> {
        let mut out = Vec::new();
        if std::mem::take(&mut self.reset_timeline) {
            // 期間を変えたら時間軸だけ左端へ。**縦は動かさない。**
            out.push(ScrollIntent::x("timeline", 0.0));
        }
        if let Some(y) = self.jump_to_row.take() {
            out.push(ScrollIntent::y("timeline", y));
        }
        out
    }
}

/// **横位置をアプリから戻せること。** これが無いと、期間を変えても時間軸が
/// 前の位置のままで、帯が画面外に居る。回避策は「読み直すたびに増える番号を
/// scroll の id に混ぜて状態ごと作り直す」で、スクロール位置の記憶が毎回
/// 捨てられていた。
#[test]
fn an_intent_can_move_the_horizontal_axis() {
    let mut h = Harness::new(Gantt::default(), 800.0, 600.0);
    h.frame();

    h.scroll_x("timeline", 900.0);
    h.frame();
    assert!(h.scroll_x_of("timeline").unwrap() > 800.0, "まず横へ動かす");

    h.app_mut().reset_timeline = true;
    h.frame();
    h.settle();

    assert_eq!(h.scroll_x_of("timeline"), Some(0.0), "左端へ戻っていない");
}

/// **中身が縮むのと同じフレームでも、頼んだ位置へ戻れること** ([#83])。
///
/// 「期間を変えたら時間軸を左端へ戻す」は、中身の幅が変わるのと位置を戻すのが
/// **同じフレーム**に来る。`set_content_width` の丸めが *今の値* を見ていたため、
/// 縮んだ直後の数フレームは値がまだ新しい最大値より右に居て、丸めが毎フレーム
/// `set_target(max)` を撃ち、アプリが頼んだ 0 を上書きしていた。`scroll_intents`
/// は 1 回しか出ないので、以後は誰も 0 に戻さず**最大値の位置で止まる**。
///
/// 縮めなければ通る ([`an_intent_can_move_the_horizontal_axis`]) ので、
/// **縮むときだけ**落ちる。
///
/// [#83]: https://github.com/Mutafika/sabitori/issues/83
#[test]
fn an_intent_survives_the_content_shrinking_in_the_same_frame() {
    let mut h = Harness::new(Gantt::default(), 800.0, 600.0);
    h.frame();

    // 3000px の時間軸を右端近くまで送る (viewport 600 → 最大 2400)。
    h.scroll_x("timeline", 2200.0);
    h.frame();
    assert_eq!(h.scroll_x_of("timeline"), Some(2200.0), "まず横へ動かす");

    // 期間を狭める = 中身が 1800px に縮む (最大 1200) のと同じフレームで左端へ。
    h.app_mut().span_px = 1800.0;
    h.app_mut().reset_timeline = true;
    h.frame();
    h.settle();

    assert_eq!(
        h.scroll_x_of("timeline"),
        Some(0.0),
        "縮んだフレームの ScrollIntent が捨てられている (#83)"
    );
}

/// **範囲の外に出た位置は、頼まれていなくても範囲内へ戻ること。** #83 の修正で
/// 丸めが見る先を「値」から「目標」へ移したので、こちらが巻き添えで消えていないか。
/// 消えると、中身が縮んだあと**誰も頼まなければ**空白を映したままになる。
#[test]
fn shrinking_the_content_still_pulls_an_out_of_range_position_back() {
    let mut h = Harness::new(Gantt::default(), 800.0, 600.0);
    h.frame();

    h.scroll_x("timeline", 2200.0);
    h.frame();

    // 縮めるだけ。ScrollIntent は出さない。
    h.app_mut().span_px = 1000.0; // 最大は 400
    h.frame();
    h.settle();

    assert_eq!(
        h.scroll_x_of("timeline"),
        Some(400.0),
        "範囲外に取り残されている"
    );
}

/// **書いた軸だけ動くこと。** 縦を指定しただけで横が左端へ飛ぶと、
/// ガント表では「行を選ぶたびに時間軸が朝に戻る」になる。
#[test]
fn an_intent_leaves_the_other_axis_alone() {
    let mut h = Harness::new(Gantt::default(), 800.0, 600.0);
    h.frame();

    h.scroll_x("timeline", 900.0);
    h.frame();
    let x_before = h.scroll_x_of("timeline").unwrap();

    h.app_mut().jump_to_row = Some(300.0);
    h.frame();
    h.settle();

    assert_eq!(h.scroll_y("timeline"), Some(300.0), "縦は動いていること");
    assert_eq!(
        h.scroll_x_of("timeline"),
        Some(x_before),
        "縦の指定で横が動いた"
    );
}

/// **表の本体に掴める帯が出る** ([#90](https://github.com/Mutafika/sabitori/issues/90))。
///
/// 本体の `.scroll` は表の内側にあり、アプリから `.scrollbar(..)` を繋げない。
/// 215 行 (issue の顧客一覧と同じ) で 12 倍あふれていても `grab` が `None` で、
/// ホイールを回し続けるしかなかった。
#[test]
fn a_long_table_body_has_a_grabbable_bar() {
    let mut h = Harness::new(TableApp::new(215), 500.0, 400.0);
    let bars = h.frame().scroll_bars();
    let bar = bars
        .iter()
        .find(|b| b.id == "files::body")
        .unwrap_or_else(|| panic!("表の本体に帯が無い: {:?}", bars));
    assert_eq!(bar.lane, sabitori::scrollbar::DEFAULT_LANE);

    // 帯の下の方を押すと、そこまで飛ぶ (掴めている)。
    let r = bar.rect;
    h.press_at(r.origin.x + r.size.width - 4.0, r.origin.y + r.size.height - 10.0);
    h.frame();
    h.release();
    assert!(h.scroll_y("files::body").unwrap() > 1000.0, "帯を押しても動かない");
    assert_eq!(h.app().state.selected, None, "帯を押したら行が選ばれた");
}

/// `scrollbar: None` なら今までどおり出さない。
#[test]
fn a_table_without_a_scrollbar_style_draws_none() {
    struct Plain(TableState);
    impl DeclarativeApp for Plain {
        fn view(&self, ctx: &ViewContext) -> Element {
            let style = TableStyle { scrollbar: None, ..TableStyle::default_dark() };
            table(ctx, "files", &self.0, &style).w_full().h_full()
        }
    }
    let mut h = Harness::new(Plain(TableApp::new(215).state), 500.0, 400.0);
    assert!(h.frame().scroll_bars().is_empty());
}

/// **帯を付けても、行の右端は行のもの** (#90 のレビューで見つけた)。
///
/// 掴める帯は右端 14px の押しを食う。行の右端に帯を重ねると、`table_with` で
/// 行末に置いたボタン (「編集」など) の右側を押しても帯が掴まれる。
/// 見出しと本体の列もずれないこと。
#[test]
fn the_table_bar_does_not_cover_the_rows_and_columns_stay_aligned() {
    let mut h = Harness::new(TableApp::new(215), 500.0, 400.0);
    let bars = h.frame().scroll_bars();
    let bar = bars.iter().find(|b| b.id == "files::body").expect("帯が無い");
    let bar_left = bar.rect.origin.x + bar.rect.size.width - bar.lane;

    let row = h.rect_of("files::row:0").expect("行が無い");
    assert!(
        bar_left >= row.origin.x + row.size.width - 0.5,
        "帯の掴める所 (x >= {bar_left}) が行 (右端 {}) に被っている",
        row.origin.x + row.size.width
    );

    let head = h.rect_of("files::col:1").expect("見出しが無い");
    let right = |r: sabitori::Rect| r.origin.x + r.size.width;
    assert!(
        (right(head) - right(row)).abs() <= 0.5,
        "見出しと行の右端がずれている: {} / {}",
        right(head),
        right(row)
    );
}
