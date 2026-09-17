//! 非同期の結果がランタイム越しに UI へ戻ること (#64)。
//!
//! 見たいのは「動く」ことではなく、**アプリが手で受け皿を書かなくて済む**
//! ことと、**テストが結果を待てる**こと。これまでは
//!
//! - `spawn` → 自前の inbox に積む → `poll_dirty()` で drain → 再描画を要求
//! - `Harness` は `poll_dirty` を呼ばないので、アプリに `#[cfg(test)] pump()`
//!
//! を 2 つのアプリが別々に書いていた。

use sabitori::testing::Harness;
use sabitori::*;

#[derive(Default)]
struct Dashboard {
    tasks: Tasks<Dashboard>,
    loading: bool,
    rows: Vec<String>,
    error: Option<String>,
}

impl DeclarativeApp for Dashboard {
    fn tasks(&self) -> Option<&Tasks<Self>> {
        Some(&self.tasks)
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        div().w_full().h_full().flex_col().children([
            div()
                .w(Px(100.0))
                .h(Px(30.0))
                .click(ctx, "reload", |app: &mut Dashboard| {
                    app.loading = true;
                    // 実アプリでは API クライアントを clone して渡す。
                    app.tasks.spawn(
                        async { Ok::<Vec<String>, String>(vec!["R-0042".into(), "R-0043".into()]) },
                        |app: &mut Dashboard, res| {
                            app.loading = false;
                            match res {
                                Ok(rows) => app.rows = rows,
                                Err(e) => app.error = Some(e),
                            }
                        },
                    );
                }),
            text(if self.loading {
                "読み込み中…".to_string()
            } else {
                format!("{} 件", self.rows.len())
            }),
        ])
    }
}

/// 押す → 走る → 結果が当たる → 画面が変わる、が `pump()` 無しで通ること。
#[test]
fn a_spawned_task_lands_in_the_ui() {
    let mut h = Harness::new(Dashboard::default(), 400.0, 300.0);
    h.frame();

    h.click("reload");
    assert!(h.app().loading, "押した直後は読み込み中");

    h.run_until_idle();

    assert!(!h.app().loading);
    assert_eq!(h.app().rows.len(), 2);
    assert!(h.app().error.is_none());
}

/// **結果が来るまでは前の状態のまま。** `run_until_idle` を呼ばずに
/// フレームだけ回しても、勝手に完了扱いにならないこと。
#[test]
fn nothing_lands_before_the_task_finishes() {
    let mut h = Harness::new(Dashboard::default(), 400.0, 300.0);
    h.frame();
    h.app_mut().loading = true;

    // タスクを投げずにフレームだけ回す。
    h.frame();
    h.frame();

    assert!(h.app().loading, "投げていないのに完了している");
    assert!(h.app().rows.is_empty());
}

/// 画面を離れたら、前の画面の応答は当たらないこと。
#[test]
fn leaving_the_screen_drops_the_answer() {
    let mut h = Harness::new(Dashboard::default(), 400.0, 300.0);
    h.frame();

    h.click("reload");
    h.app().tasks.cancel_all();
    h.run_until_idle();

    assert!(h.app().rows.is_empty(), "捨てたはずの結果が当たっている");
}

/// 何度押しても、来た順に全部当たること (取りこぼさない)。
#[test]
fn every_answer_lands() {
    let mut h = Harness::new(Dashboard::default(), 400.0, 300.0);
    h.frame();

    for _ in 0..5 {
        h.click("reload");
    }
    h.run_until_idle();

    assert_eq!(h.app().rows.len(), 2);
    assert!(!h.app().loading);
}
