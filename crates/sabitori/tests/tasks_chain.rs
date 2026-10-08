//! `Harness::run_until_idle` は、タスクの結果を当てた処理が**次のタスクを投げる**
//! 連鎖まで待つ。
//!
//! 「操作 → 終わったら一覧を読み直す」というファイラの形で、読み直しの結果が
//! 当たる前に戻ってしまい、テストが古い一覧を見ていた。結果が「届いた」と
//! 「当てた」の間に居るところで `pending() == 0` を見て止まっていた。

use sabitori::testing::Harness;
use sabitori::*;

#[derive(Default)]
struct Chain {
    tasks: Tasks<Chain>,
    /// 1 段目の結果。
    first: Option<u32>,
    /// 1 段目を受けて投げた 2 段目の結果。
    second: Option<u32>,
}

impl Chain {
    fn start(&mut self) {
        self.tasks.spawn(async { 1 }, |app, v| {
            app.first = Some(v);
            // 結果を受けて、さらに読み直す (lustar の Job → 一覧の形)
            app.tasks.spawn(async { 2 }, |app, v| app.second = Some(v));
        });
    }
}

impl DeclarativeApp for Chain {
    fn tasks(&self) -> Option<&Tasks<Self>> {
        Some(&self.tasks)
    }

    fn view(&self, _ctx: &ViewContext) -> Element {
        div()
    }
}

#[test]
fn run_until_idle_waits_for_tasks_spawned_by_results() {
    // 競合は「届く」と「数える」の間の一瞬なので、回数で踏む
    for _ in 0..200 {
        let mut h = Harness::new(Chain::default(), 200.0, 100.0);
        h.app_mut().start();
        h.run_until_idle();
        assert_eq!(h.app().first, Some(1));
        assert_eq!(h.app().second, Some(2), "2 段目の結果まで当たってから戻る");
        assert!(h.app().tasks.is_idle());
    }
}

#[test]
fn is_idle_sees_results_waiting_to_be_applied() {
    let tasks: Tasks<Chain> = Tasks::new();
    assert!(tasks.is_idle());
    tasks.spawn(async { 7 }, |app, v| app.first = Some(v));
    assert!(!tasks.is_idle(), "走っている間は idle ではない");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while tasks.pending() > 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        !tasks.is_idle(),
        "届いたが当てていない結果がある間も idle ではない"
    );
}
