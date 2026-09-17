//! 非同期の結果を UI に戻す ([#64](https://github.com/Mutafika/sabitori/issues/64))。
//!
//! 「ボタンを押す → 裏で API を呼ぶ → 結果で状態を更新して再描画」という
//! 定番の形が、これまでフレームワークに無かった。2 つのアプリが同じものを
//! 別々に手書きしていた:
//!
//! - wasm: `spawn_local` → `Rc<RefCell<Vec<Msg>>>` に積む → `tick()` で取り出す
//! - native: `std::thread::spawn` → `mpsc` に送る → `poll_dirty()` で drain
//!
//! しかも `Harness` は `poll_dirty` を呼ばないので、**テストからは結果を
//! 待てなかった** (アプリ側に `#[cfg(test)]` の `pump()` が生えていた)。
//!
//! # 使い方
//!
//! `Tasks<App>` をアプリが 1 つ持ち、[`DeclarativeApp::tasks`] で渡す。
//! あとはどこからでも `spawn` できる:
//!
//! ```ignore
//! struct App { tasks: Tasks<App>, dashboard: Option<Dashboard>, loading: bool }
//!
//! impl DeclarativeApp for App {
//!     fn tasks(&self) -> Option<&Tasks<Self>> { Some(&self.tasks) }
//! }
//!
//! // クリック処理の中 (引数は `&mut App` のまま — 形は何も変わらない)
//! button("再読込").click(ctx, "reload", |app: &mut App| {
//!     app.loading = true;
//!     let api = app.api.clone();
//!     app.tasks.spawn(async move { api.dashboard().await }, |app, res| {
//!         app.loading = false;
//!         app.dashboard = res.ok();
//!     });
//! })
//! ```
//!
//! 終わったら**自動で再描画される**。`poll_dirty` を書く必要は無い。
//!
//! # 実行のされ方
//!
//! | | 走る場所 | 要件 |
//! |---|---|---|
//! | native | スレッド 1 本 + [`pollster`] | `Future` と結果が `Send` |
//! | wasm | `spawn_local` (同じスレッド) | `Send` は要らない |
//!
//! native に tokio を持ち込んでいないのは、**GUI の道具が非同期ランタイムを
//! 選んでしまう**のを避けるため。スレッドの中で待つので、待ち方が「IO を
//! ブロックする」形でも動く (`reqwest::blocking` など)。tokio の reactor を
//! 要する future (tokio の timer / socket) を直接渡すことはできない —
//! その場合はアプリ側で tokio を持ち、その `Runtime` の中で完結させること。
//!
//! # 画面を離れたら捨てる
//!
//! [`Tasks::cancel_all`] で**世代**が進み、それ以前に投げたものの結果は
//! 捨てられる。画面を切り替えるときに呼べば、前の画面の応答が新しい画面の
//! 状態を踏み荒らさない。

use std::future::Future;
use std::sync::{Arc, Mutex};

/// 結果が届いたときにアプリへ当てる処理。
///
/// native はスレッドから渡ってくるので `Send` が要る。wasm は同じスレッドで
/// 完結するので要らない (`JsValue` を掴んだ結果も返せる)。
#[cfg(not(target_arch = "wasm32"))]
type Apply<A> = Box<dyn FnOnce(&mut A) + Send>;
#[cfg(target_arch = "wasm32")]
type Apply<A> = Box<dyn FnOnce(&mut A)>;

struct Inbox<A> {
    ready: Vec<(u64, Apply<A>)>,
    pending: usize,
    generation: u64,
}

impl<A> Default for Inbox<A> {
    fn default() -> Self {
        Self { ready: Vec::new(), pending: 0, generation: 0 }
    }
}

/// アプリが 1 つ持つ、非同期タスクの受け皿。
///
/// 中身は共有ハンドルなので `clone()` は安く、どこへ持ち回してもよい。
pub struct Tasks<A> {
    inbox: Arc<Mutex<Inbox<A>>>,
}

impl<A> Clone for Tasks<A> {
    fn clone(&self) -> Self {
        Self { inbox: Arc::clone(&self.inbox) }
    }
}

impl<A: 'static> Default for Tasks<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: 'static> Tasks<A> {
    pub fn new() -> Self {
        Self { inbox: Arc::new(Mutex::new(Inbox::default())) }
    }

    /// まだ結果を待っているタスクの数。
    pub fn pending(&self) -> usize {
        self.inbox.lock().map(|i| i.pending).unwrap_or(0)
    }

    /// 今走っているタスクの結果を**全部捨てる**。
    ///
    /// タスク自体は止まらない (止められる保証のある形にすると、待ち方に
    /// 制約が出る)。届いた結果を当てないだけ。画面を離れるときに呼ぶ。
    pub fn cancel_all(&self) {
        if let Ok(mut inbox) = self.inbox.lock() {
            inbox.generation += 1;
            inbox.ready.clear();
        }
    }

    /// 結果を当てる処理を積む。プラットフォーム実装から呼ぶ。
    fn deliver(inbox: &Arc<Mutex<Inbox<A>>>, generation: u64, apply: Apply<A>) {
        if let Ok(mut i) = inbox.lock() {
            i.pending = i.pending.saturating_sub(1);
            if i.generation == generation {
                i.ready.push((generation, apply));
            }
        }
        #[cfg(target_arch = "wasm32")]
        crate::web_wake::wake();
    }

    /// 溜まった結果を取り出す。ランタイムが毎フレーム呼ぶ。
    pub(crate) fn drain(&self) -> Vec<Apply<A>> {
        let Ok(mut inbox) = self.inbox.lock() else {
            return Vec::new();
        };
        let current = inbox.generation;
        let ready = std::mem::take(&mut inbox.ready);
        ready
            .into_iter()
            .filter(|(g, _)| *g == current)
            .map(|(_, a)| a)
            .collect()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<A: 'static> Tasks<A> {
    /// 裏で走らせて、終わったら `on_done` をアプリに当てる。
    ///
    /// native ではスレッドを 1 本使って [`pollster`] で待つ。
    pub fn spawn<F, T>(&self, fut: F, on_done: impl FnOnce(&mut A, T) + Send + 'static)
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let inbox = Arc::clone(&self.inbox);
        let generation = {
            let Ok(mut i) = self.inbox.lock() else { return };
            i.pending += 1;
            i.generation
        };
        std::thread::spawn(move || {
            let out = pollster::block_on(fut);
            Self::deliver(&inbox, generation, Box::new(move |app: &mut A| on_done(app, out)));
        });
    }
}

#[cfg(target_arch = "wasm32")]
impl<A: 'static> Tasks<A> {
    /// 裏で走らせて、終わったら `on_done` をアプリに当てる。
    ///
    /// wasm では `spawn_local` — 同じスレッドなので `Send` は要らない。
    pub fn spawn<F, T>(&self, fut: F, on_done: impl FnOnce(&mut A, T) + 'static)
    where
        F: Future<Output = T> + 'static,
        T: 'static,
    {
        let inbox = Arc::clone(&self.inbox);
        let generation = {
            let Ok(mut i) = self.inbox.lock() else { return };
            i.pending += 1;
            i.generation
        };
        wasm_bindgen_futures::spawn_local(async move {
            let out = fut.await;
            Self::deliver(&inbox, generation, Box::new(move |app: &mut A| on_done(app, out)));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct App {
        log: Vec<String>,
    }

    fn apply_all(tasks: &Tasks<App>, app: &mut App) {
        for f in tasks.drain() {
            f(app);
        }
    }

    fn settle(tasks: &Tasks<App>) {
        for _ in 0..2000 {
            if tasks.pending() == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("タスクが終わらない");
    }

    /// 結果がアプリに当たること。
    #[test]
    fn a_finished_task_applies_to_the_app() {
        let tasks: Tasks<App> = Tasks::new();
        let mut app = App::default();

        tasks.spawn(async { 42 }, |app: &mut App, n: i32| app.log.push(format!("got {n}")));
        settle(&tasks);
        apply_all(&tasks, &mut app);

        assert_eq!(app.log, vec!["got 42"]);
    }

    /// **1 回だけ当たること。** 2 回当たると、一覧が二重に積まれる。
    #[test]
    fn a_result_applies_once() {
        let tasks: Tasks<App> = Tasks::new();
        let mut app = App::default();

        tasks.spawn(async { 1 }, |app: &mut App, n: i32| app.log.push(n.to_string()));
        settle(&tasks);
        apply_all(&tasks, &mut app);
        apply_all(&tasks, &mut app);

        assert_eq!(app.log.len(), 1);
    }

    /// 画面を離れたら、前の画面の応答は捨てられること。
    #[test]
    fn cancel_all_drops_results_in_flight() {
        let tasks: Tasks<App> = Tasks::new();
        let mut app = App::default();

        tasks.spawn(async { "古い画面" }, |app: &mut App, s: &'static str| {
            app.log.push(s.to_string())
        });
        settle(&tasks);
        tasks.cancel_all();
        apply_all(&tasks, &mut app);

        assert!(app.log.is_empty(), "捨てたはずの結果が当たっている");

        // 以後に投げたものは当たる。
        tasks.spawn(async { "新しい画面" }, |app: &mut App, s: &'static str| {
            app.log.push(s.to_string())
        });
        settle(&tasks);
        apply_all(&tasks, &mut app);
        assert_eq!(app.log, vec!["新しい画面"]);
    }

    /// 走っている数が分かること (`run_until_idle` の土台)。
    #[test]
    fn pending_counts_what_is_still_running() {
        let tasks: Tasks<App> = Tasks::new();
        assert_eq!(tasks.pending(), 0);

        tasks.spawn(async { () }, |_app: &mut App, _| {});
        settle(&tasks);
        assert_eq!(tasks.pending(), 0, "終わったら 0 に戻ること");
    }
}
