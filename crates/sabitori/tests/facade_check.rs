//! **`use sabitori::*;` だけで新しい API に届くこと。**
//!
//! ファサードは `sabitori_core::*` と `sabitori_widgets::*` を glob で通しているが、
//! 通っていない口が混ざっていても**コンパイルは通る** (利用側で初めて失敗する)。
//! 実際 `polyline()` は `sabitori::element::polyline` と書かないと使えず、
//! 業務アプリ側で回避を書いていた ([#75] の 10)。
//!
//! ここは「アプリが書くとおりに 1 回書いてみる」テスト。名前を移動したときに、
//! 利用側より先にここが落ちる。
//!
//! [#75]: https://github.com/Mutafika/sabitori/issues/75

use sabitori::*;

#[test]
fn everything_an_app_writes_is_reachable_from_the_facade() {
    // 図形 (#75 の 10)
    let _line = polyline()
        .points_normalized([(0.0, 0.0), (1.0, 1.0)])
        .stroke_width(2.0)
        .stroke_color(Color::WHITE);
    let _arc = arc().arc_value(0.5).arc_colors(Color::WHITE, Color::BLACK);

    // 浮かせる / 固定する (#75 の 2・4)
    let _floating = div().anchor_to("trigger", Placement::Below).anchor_match_width();
    let _pinned = div().sticky_x().sticky_y();

    // 日時 (#75 の 1)
    let mut time = TimePickerState::new("t", 9, 0).with_minute_step(15).with_hour_range(8, 19);
    assert!(time.set_from_str("10:30"));
    let _dt = DateTimePickerState::new("pickup", 2026, 6, 10, 9, 0);
    assert_eq!(parse_hhmm("0930"), Some((9, 30)));

    // モーダル (#75 の 7・16)
    let modal_state = ModalState::new();
    assert!(!modal_state.is_open());
    let _builder: fn(&ViewContext, &str, &ModalState, &ModalStyle, &str, Vec<Element>) -> Option<Element> =
        modal;

    // 表 (#75 の 3)
    let _cell = Cell::text("R-0042");

    // 非同期と HTTP (#64 / #63)
    let _tasks: Tasks<()> = Tasks::new();

    // 画面外に描く / 実行時フォント (#75 の 12・13)
    let sheet = offscreen::Sheet::a4().dpi(300.0);
    assert_eq!(sheet.pixel_size(), (2480, 3508));
    let before = fonts::count();
    fonts::add(Vec::new()); // 空は積まれない
    assert_eq!(fonts::count(), before);
}

/// ファイルを選ぶ・保存する (#77) と、テーマ (#65)。
#[test]
fn the_platform_doors_are_reachable_too() {
    let _light = AppTheme::light();
    assert!(!AppTheme::light().is_dark());
    let _opts = files::PickOptions::default();
}
