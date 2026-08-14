//! `sabitori::testing::Harness` を **crate の外から**使う。
//!
//! 単体テストではなく integration test なのが要点。 消費側とまったく同じ解決経路を
//! 通るので、 「crate 内からは書けるが外からは書けない」 という漏れを検出できる
//! (`tests/facade.rs` と同じ理由)。
//!
//! 中身は「消費側が書きたくなるであろうテスト」をそのまま書いてある。 読み物として
//! の例も兼ねる。

use sabitori::testing::Harness;
use sabitori::{div, text, Element, Key, Modifiers, Px, ViewContext};
use sabitori_widgets::{TextInputState, TextInputStyle};

/// 名前を入れて保存する、それだけのアプリ。
#[derive(Default)]
struct Form {
    name: TextInputState,
    saved: Option<String>,
    rows: usize,
}

impl Form {
    fn new(rows: usize) -> Self {
        Self {
            name: TextInputState::new("名前"),
            saved: None,
            rows,
        }
    }

    fn style() -> TextInputStyle {
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
}

impl sabitori::DeclarativeApp for Form {
    fn view(&self, ctx: &ViewContext) -> Element {
        let rows: Vec<Element> = (0..self.rows)
            .map(|i| div().id(format!("row-{i}")).w_full().h(Px(40.0)))
            .collect();

        div().flex_col().w_full().h_full().children([
            sabitori_widgets::text_input(ctx, "name", &self.name, &Self::style()),
            div()
                .id("save")
                .w(Px(80.0))
                .h(Px(32.0))
                .on_click(|| {})
                .child(text("保存")),
            div().scroll("list").flex_1().flex_col().children(rows),
        ])
    }

    fn on_click(&mut self, id: &str) {
        if id == "save" {
            self.saved = Some(self.name.text());
        }
    }

    // 0.4.0 以降、 テキスト欄への配線は要らない。 `text_input(..)` を `view()`
    // に置いた時点でランタイムが配信と tick を引き受ける。
}

/// クリックがアプリのハンドラまで届くこと。 いちばん基本の形。
#[test]
fn clicking_a_button_runs_its_handler() {
    let mut h = Harness::new(Form::new(0), 400.0, 400.0);
    h.frame();

    h.click("save");

    assert_eq!(h.app().saved.as_deref(), Some(""));
}

/// テキスト欄にフォーカスして打った文字が state に入ること。
///
/// 「フォーカスして打つ」 は #16 で統合した `text_input` の中心的な用途なのに、
/// 0.4.0 より前はこれを自動で確かめる手段が無かった。
#[test]
fn typing_into_a_focused_field_updates_its_state() {
    let mut h = Harness::new(Form::new(0), 400.0, 400.0);
    h.frame();

    h.click("name"); // focusable なのでフォーカスが入る
    h.text("abc");

    assert_eq!(h.app().name.text(), "abc");
    assert_eq!(h.app().name.cursor_pos(), 3);
}

/// 打った内容が保存に反映されること（フォーカス経路とクリック経路の合流）。
#[test]
fn typed_text_reaches_the_save_handler() {
    let mut h = Harness::new(Form::new(0), 400.0, 400.0);
    h.frame();

    h.click("name");
    h.text("kubo");
    h.frame(); // 表示を更新（保存ボタンの位置取りのため）
    h.click("save");

    assert_eq!(h.app().saved.as_deref(), Some("kubo"));
}

/// Tab でフォーカスが移ること。 移動先は `capture()` から観測できる。
#[test]
fn tab_moves_focus_between_focusable_elements() {
    let mut h = Harness::new(Form::new(0), 400.0, 400.0);
    h.frame();

    h.key(Key::Tab, Modifiers::default());

    assert_eq!(
        h.focused_id(),
        Some("name"),
        "唯一の focusable にフォーカスが入る"
    );
}

/// 管理スクロールが動き、 画面外の行が hit_regions から落ちること。
///
/// **`hit_regions` は見えているものだけ**なので、 スクロールで隠れた行は
/// `visible_ids()` から消える。 消費側が「どこまで見えているか」 を assert できる。
#[test]
fn scrolling_changes_which_rows_are_visible() {
    let mut h = Harness::new(Form::new(50), 400.0, 400.0);
    h.frame();

    let before = h.visible_ids();
    assert!(before.iter().any(|id| id == "row-0"), "最初は先頭が見えている");

    h.scroll("list", 600.0);
    h.frame();

    let after = h.visible_ids();
    assert!(
        !after.iter().any(|id| id == "row-0"),
        "スクロール後は先頭が画面外へ (見えているのは {after:?})"
    );
    assert!(
        h.scroll_y("list").unwrap_or(0.0) > 0.0,
        "スクロール位置が進んでいること"
    );
}

/// `.scroll_manual` のコンテナはランタイム管理ではないので、 `scroll()` は無視される。
/// #14 で分けた 2 モデルの区別が、 消費側から観測できることの確認。
#[test]
fn manual_scroll_containers_are_not_driven_by_the_harness() {
    struct Manual;
    impl sabitori::DeclarativeApp for Manual {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div().id("m").scroll_manual(0.0, 0.0).w_full().h_full()
        }
    }
    let mut h = Harness::new(Manual, 400.0, 400.0);
    h.frame();

    h.scroll("m", 500.0);

    assert_eq!(
        h.scroll_y("m"),
        None,
        "アプリ所有のコンテナに管理状態を作ってはいけない"
    );
}

/// ペーストがフォーカス中の欄に入ること。
///
/// ランタイムの Cmd/Ctrl+V は「ショートカット判定 → クリップボード読み →
/// `Paste` 配信」の 3 段で、 ここは最後の配信を再現している。 実クリップボードは
/// 環境依存なのでテストから外してある。
#[test]
fn paste_reaches_the_focused_field() {
    let mut h = Harness::new(Form::new(0), 400.0, 400.0);
    h.frame();

    h.click("name");
    h.paste("https://example.com/a?b=1");

    assert_eq!(h.app().name.text(), "https://example.com/a?b=1");
}

/// フォーカスが無ければ欄には入らない（アプリの `on_input` には届く）。
#[test]
fn paste_without_focus_does_not_touch_the_field() {
    let mut h = Harness::new(Form::new(0), 400.0, 400.0);
    h.frame();

    h.paste("xyz");

    assert_eq!(h.app().name.text(), "");
}

// ─────────────────────────────────────────────────────────────────────
// `desired_focus` — クリックさせずに欄を掴む (#28)
// ─────────────────────────────────────────────────────────────────────

/// コマンドパレット。 開いている間だけ、 中の入力欄にフォーカスを主張する。
///
/// `desired_focus` の代表的な用途そのもの — 開いた瞬間から打てる。
struct Palette {
    q: TextInputState,
    open: bool,
}

impl Palette {
    fn new(open: bool) -> Self {
        Self {
            q: TextInputState::new(""),
            open,
        }
    }
}

impl sabitori::DeclarativeApp for Palette {
    fn view(&self, ctx: &ViewContext) -> Element {
        let mut root = div().flex_col().w_full().h_full().child(
            div()
                .id("open")
                .w(Px(80.0))
                .h(Px(32.0))
                .on_click(|| {})
                .child(text("開く")),
        );
        // パレット外の focusable。 フォーカスを奪ってみる相手役。
        root = root.child(
            div()
                .id("side")
                .w(Px(80.0))
                .h(Px(32.0))
                .focusable()
                .child(text("横")),
        );
        if self.open {
            root = root.child(sabitori_widgets::text_input(
                ctx,
                "q",
                &self.q,
                &Form::style(),
            ));
        }
        root
    }

    fn on_click(&mut self, id: &str) {
        if id == "open" {
            self.open = true;
        }
    }

    fn desired_focus(&self) -> Option<String> {
        self.open.then(|| "q".to_string())
    }
}

/// **これが #28 の本体。** `desired_focus` を実装したアプリを Harness に載せて、
/// クリックせずに打てること。
///
/// v0.5.0 までは反映が `about_to_wait` にベタ書きされていたので、 Harness からは
/// 一度も通らなかった。 実機では動くのにテストだけが落ちる — しかも
/// `focused_id()` が `None` のままなので、 何が起きていないのかも見えなかった。
#[test]
fn desired_focus_grabs_the_field_without_a_click() {
    let mut h = Harness::new(Palette::new(true), 400.0, 400.0);

    h.frame();

    assert_eq!(
        h.focused_id(),
        Some("q"),
        "アプリが主張したフォーカスが、 最初のフレームで入っていること"
    );

    h.text("abc");

    assert_eq!(h.app().q.text(), "abc", "クリックせずに打てること");
}

/// 掴んだフレームの `view()` からもう見えていること。
///
/// `build_frame` の**頭**で当てているので、 掴んだその同じフレームの
/// `ctx.focused` に出る。 フォーカス枠が 1 フレーム遅れて光らない。
#[test]
fn the_grabbed_focus_is_visible_to_the_same_frame() {
    struct Probe {
        seen: std::cell::Cell<Option<String>>,
    }
    impl sabitori::DeclarativeApp for Probe {
        fn view(&self, ctx: &ViewContext) -> Element {
            self.seen.set(ctx.focused.clone());
            div().id("q").w(Px(80.0)).h(Px(32.0)).focusable()
        }
        fn desired_focus(&self) -> Option<String> {
            Some("q".to_string())
        }
    }

    let h_app = Probe {
        seen: std::cell::Cell::new(None),
    };
    let mut h = Harness::new(h_app, 400.0, 400.0);
    h.frame();

    assert_eq!(
        h.app().seen.take().as_deref(),
        Some("q"),
        "掴んだ結果は、 その掴んだフレームの ctx.focused に出ていること"
    );
}

/// 実際の使い方 — ボタンで開いて、そのまま打つ。
#[test]
fn opening_the_palette_moves_focus_into_it() {
    let mut h = Harness::new(Palette::new(false), 400.0, 400.0);
    h.frame();
    assert_eq!(h.focused_id(), None, "閉じている間は誰も掴まない");

    h.click("open");
    h.frame();

    assert_eq!(h.focused_id(), Some("q"));
    h.text("hi");
    assert_eq!(h.app().q.text(), "hi");
}

/// `frame()` を呼ばず `tick()` だけでも反映されること。
///
/// 実機の cadence は `about_to_wait`（= tick）→ 描画なので、 lazy_render で
/// 描画が止まっている間もフォーカスは追随する。 その経路が Harness にもある。
#[test]
fn desired_focus_also_applies_on_a_bare_tick() {
    let mut h = Harness::new(Palette::new(false), 400.0, 400.0);
    h.frame();

    h.app_mut().open = true;
    h.tick(0.016);

    assert_eq!(h.focused_id(), Some("q"));
}

/// `None` の間はランタイムがフォーカスに触らないこと。
#[test]
fn desired_focus_of_none_leaves_focus_alone() {
    let mut h = Harness::new(Palette::new(false), 400.0, 400.0);
    h.frame();

    h.click("side");
    h.frame();

    assert_eq!(
        h.focused_id(),
        Some("side"),
        "主張が無いなら、 クリックで入ったフォーカスはそのまま"
    );
}

/// 主張し続けている間は引き戻すこと（既存の挙動を固定する）。
///
/// 「モーダルが開いている間はそこから出さない」ための性質で、 `Some` を返す
/// 限り毎フレーム主張し直す。
#[test]
fn desired_focus_reasserts_itself_after_focus_moves_away() {
    let mut h = Harness::new(Palette::new(true), 400.0, 400.0);
    h.frame();
    assert_eq!(h.focused_id(), Some("q"));

    h.click("side");
    assert_eq!(h.focused_id(), Some("side"), "クリックの瞬間は移る");

    h.frame();

    assert_eq!(
        h.focused_id(),
        Some("q"),
        "次のフレームでアプリが引き戻すこと"
    );
}

/// 掴んだら `wants_keyboard` も立つこと。
///
/// フォーカスだけ動いて capture が古いままだと、 埋め込みホストは「UI は
/// キーボードを要らない」と判断して打鍵を自分で食ってしまう。
#[test]
fn grabbing_focus_updates_the_capture_snapshot() {
    let mut h = Harness::new(Palette::new(true), 400.0, 400.0);
    h.frame();

    assert!(
        h.capture().wants_keyboard,
        "掴んだ以上、 キーボードは UI が要る"
    );
}

/// レイアウトが変わったら `capture()` も追随すること。
///
/// `wants_pointer` は「いまのマウス位置の下に UI があるか」なので、 マウスが
/// 動かなくても**ツリーが変われば答えが変わる**。 実機のランタイムは毎 tick
/// 押し直しているが、 Harness にその経路が無いと、 消費側は古い答えを見る。
#[test]
fn the_capture_snapshot_follows_layout_changes() {
    struct Panel {
        show: bool,
    }
    impl sabitori::DeclarativeApp for Panel {
        fn view(&self, _ctx: &ViewContext) -> Element {
            if self.show {
                div().id("panel").w_full().h_full().on_click(|| {})
            } else {
                div()
            }
        }
    }

    let mut h = Harness::new(Panel { show: true }, 400.0, 400.0);
    h.frame();
    h.move_to(200.0, 200.0);
    assert!(h.capture().wants_pointer, "パネルの上にいる");

    h.app_mut().show = false;
    h.frame();

    assert!(
        !h.capture().wants_pointer,
        "パネルが消えたら、 UI はもうポインタを要らない"
    );
}
