//! sabitori で書いたアプリに、 窓も GPU も無しで回帰テストを書くための足場。
//!
//! # なぜ要るか
//!
//! 0.4.0 より前、 消費側が使えたのは `build_tree_measured`（純粋なレイアウト）だけ
//! だった。 「このボタンを押したら state がこう変わる」「Tab を押したらフォーカスが
//! ここに移る」 は書けない。 ランタイムはヘッドレスでフレームを回せる作りに
//! なっていたのに、 その入口が `#[cfg(test)]` の中に閉じていた (issue #19)。
//!
//! 実際 issue #1 / #3 / #12 / #14 は**全部「人間が手で動かして気づいた」**で
//! 見つかっている。 いずれも見た目に異常が出ない「黙って効かない」タイプで、
//! 手動確認では見落としやすい。
//!
//! # 使い方
//!
//! ```ignore
//! use sabitori::testing::Harness;
//!
//! let mut h = Harness::new(MyApp::default(), 800.0, 600.0);
//! h.frame();                       // 1 フレーム回す
//! h.click("save");                 // id を指定してクリック
//! assert!(h.app().saved);
//!
//! h.key(Key::Tab, Modifiers::default());
//! assert_eq!(h.focused_id(), Some("name"));
//! ```
//!
//! # 精度について
//!
//! テキストの計測は実フォントではなく決め打ちのスタブ（1 文字 = `font_size * 0.5`
//! 幅、 1 行 = `font_size` 高）。 環境にインストールされた書体に依存しないので
//! 期待値を手で書けるが、 **実物の折り返し位置とは一致しない**。 レイアウトの
//! ピクセル値そのものを assert する用途には向かない。 「どの要素が居るか」
//! 「どの id がクリックされたか」「state がどう変わったか」を見ること。
//!
//! ヘッドレスなので GPU 描画・IME・実際の winit イベントは通らない。 IME 合成の
//! ような OS 依存の経路はここでは再現できない。

use sabitori_core::build::{BuildResult, TextMeasure};
use sabitori_core::{Element, Size, TextMetrics, Typography};
use sabitori_input::{Key, Modifiers};

use crate::declarative::{AppState, DeclarativeApp, UiCapture};

/// 決め打ちのテキスト計測。
///
/// 実フォントで測ると、 期待値がマシンにインストールされた書体に依存してしまう。
/// ここでは 1 文字 = `font_size * 0.5` 幅、 1 行 = `font_size` 高で固定する。
/// 折り返しは模していない。
pub struct StubMeasure;

impl TextMeasure for StubMeasure {
    fn measure(
        &self,
        content: &str,
        font_size: f32,
        _bold: bool,
        _monospace: bool,
        _font_family: Option<&str>,
        _max_width: Option<f32>,
        _max_lines: Option<u32>,
        _typo: Typography,
    ) -> TextMetrics {
        TextMetrics {
            size: Size {
                width: content.chars().count() as f32 * font_size * 0.5,
                height: font_size,
            },
            baseline: font_size * 0.8,
        }
    }
}

/// [`DeclarativeApp`] をヘッドレスで駆動する。
///
/// 窓も wgpu デバイスも作らない。 モジュールの doc も参照。
pub struct Harness<A: DeclarativeApp> {
    state: AppState<A>,
    width: f32,
    height: f32,
}

impl<A: DeclarativeApp> Harness<A> {
    /// ビューポートサイズを決めてアプリを載せる。 まだフレームは回らない。
    pub fn new(app: A, width: f32, height: f32) -> Self {
        Self {
            state: AppState::new(app),
            width,
            height,
        }
    }

    /// 時間を `dt` 秒進める。
    ///
    /// アプリの `tick` と、 ランタイムのアニメーション (スクロールのばね・慣性、
    /// tooltip の遅延、 style / presence) が進む。 実装はランタイムと同じ
    /// `AppState::advance` を通るので、 tick 対象が増えても勝手に付いてくる。
    ///
    /// **ばねを使う挙動はこれを呼ばないと動かない。** 代表例が
    /// `scroll_intents()` — あれは `smooth_scroll_to` で目標を置くだけなので、
    /// 時間を進めないと位置は 1px も変わらない。
    ///
    /// ```ignore
    /// h.app_mut().pending_scroll = Some(0.0);
    /// h.frame();          // intent がランタイムへ渡る
    /// h.settle();         // ばねが目標に着くまで進める
    /// assert_eq!(h.scroll_y("list"), Some(0.0));
    /// ```
    pub fn tick(&mut self, dt: f32) {
        self.state.advance(dt);
    }

    /// アニメーションが落ち着くまでフレームを回す。
    ///
    /// 1 フレーム 16ms 換算で最大 `max_frames` 回。 ばねは漸近するので
    /// 「完全静止」は待たず、 実用上ふつうの上限で打ち切る。 何フレーム
    /// 回ったかを返す。
    pub fn settle_for(&mut self, max_frames: usize) -> usize {
        for i in 0..max_frames {
            self.tick(0.016);
            self.frame();
            if !self.state.is_animating() {
                return i + 1;
            }
        }
        max_frames
    }

    /// [`Self::settle_for`] を既定の上限 (120 フレーム ≒ 2 秒) で回す。
    pub fn settle(&mut self) -> usize {
        self.settle_for(120)
    }

    /// ビューポートサイズを変える。 次の [`Self::frame`] から効く。
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }

    /// 1 フレーム回す。
    ///
    /// ツリーを組み、 レイアウトし、 描画された結果としてコミットするところまで。
    /// クリックやキー入力は**直前のフレームの hit_regions**を見るので、 操作の前に
    /// 最低 1 回呼ぶこと。
    pub fn frame(&mut self) -> &BuildResult {
        let frame = self.state.build_frame(self.width, self.height, &StubMeasure);
        self.state.commit_build(frame.build_result);
        self.build()
    }

    /// 直近のフレームのビルド結果。 [`Self::frame`] を 1 度も呼んでいなければ panic。
    pub fn build(&self) -> &BuildResult {
        self.state
            .last_build
            .as_ref()
            .expect("frame() を先に呼ぶこと — ヒット領域が無いと操作を流せない")
    }

    /// アプリへの不変参照。 assert はここから。
    pub fn app(&self) -> &A {
        &self.state.app
    }

    /// アプリへの可変参照。 テストの前提を直に組み立てたいとき用。
    pub fn app_mut(&mut self) -> &mut A {
        &mut self.state.app
    }

    /// ランタイムがアプリへ最後に渡した [`UiCapture`]。
    ///
    /// 中身は「ポインタ / キーボードを UI が掴んでいるか」の 2 つだけ。
    /// **フォーカス中の id はここには載っていない** ので [`Self::focused_id`] を使う。
    pub fn capture(&self) -> &UiCapture {
        &self.state.last_capture
    }

    /// いまフォーカスを持っている要素の id。
    ///
    /// `UiCapture` からは引けないので直に出す (issue #19 の使用感確認で判明)。
    /// アプリ側からは `ViewContext::focused` で同じ値が見える。
    pub fn focused_id(&self) -> Option<&str> {
        self.state.focused_id.as_deref()
    }

    /// 座標を指定してポインタを動かす。 ホバーの更新まで行う。
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.state.mouse_x = x;
        self.state.mouse_y = y;
        self.state.update_hover();
    }

    /// 座標を指定して主ボタンを押して離す。
    pub fn click_at(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.state.press_primary();
        self.state.release_primary();
    }

    /// 押したままにする（ドラッグの起点）。
    pub fn press_at(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.state.press_primary();
    }

    /// 押していたボタンを離す。
    pub fn release(&mut self) {
        self.state.release_primary();
    }

    /// `id` の要素の中心をクリックする。
    ///
    /// 直近のフレームの hit_regions から引く。 **見えていない要素は引けない** —
    /// クリップで外れた要素は hit_regions に載らないため。 その場合は
    /// [`Self::rect_of`] が `None` を返し、 ここは panic する。
    pub fn click(&mut self, id: &str) {
        let (x, y) = self.center_of(id);
        self.click_at(x, y);
    }

    /// `id` の要素の矩形。 見えていなければ `None`。
    pub fn rect_of(&self, id: &str) -> Option<sabitori_core::Rect> {
        self.build()
            .hit_regions
            .iter()
            .find(|r| r.id.as_deref() == Some(id))
            .map(|r| r.rect)
    }

    fn center_of(&self, id: &str) -> (f32, f32) {
        let rect = self.rect_of(id).unwrap_or_else(|| {
            let ids: Vec<&str> = self
                .build()
                .hit_regions
                .iter()
                .filter_map(|r| r.id.as_deref())
                .collect();
            panic!("id {id:?} が見当たらない。 このフレームに居るのは {ids:?}")
        });
        (
            rect.origin.x + rect.size.width * 0.5,
            rect.origin.y + rect.size.height * 0.5,
        )
    }

    /// キーを押して離す。 押しっぱなしを作りたいなら [`Self::key_down`]。
    pub fn key(&mut self, key: Key, modifiers: Modifiers) {
        self.key_down(key, modifiers);
        self.key_up(key, modifiers);
    }

    /// キー押下。 `modifiers` はこの時点の状態としてランタイムに入る。
    pub fn key_down(&mut self, key: Key, modifiers: Modifiers) {
        self.state.set_modifiers(modifiers);
        self.state.handle_key_input(key, true, Vec::new());
    }

    /// キー解放。
    pub fn key_up(&mut self, key: Key, modifiers: Modifiers) {
        self.state.set_modifiers(modifiers);
        self.state.handle_key_input(key, false, Vec::new());
    }

    /// 修飾キーの状態を変える（`ModifiersChanged` が飛ぶ）。
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.state.set_modifiers(modifiers);
    }

    /// 文字を打つ。 IME を経由しない直接入力（`CharInput`）として届く。
    ///
    /// 日本語変換の合成は OS の IME が作るものなので、 ここでは再現できない。
    /// 変換中の挙動をテストしたいなら `TextInputState::on_ime_preedit` を直に叩くこと。
    pub fn text(&mut self, s: &str) {
        for ch in s.chars() {
            self.state
                .handle_key_input(Key::Other, true, vec![ch]);
        }
    }

    /// **打鍵が行き場を失ったテキスト欄**の id。 空なら配線は通っている。
    ///
    /// `text_input(..)` を `view()` に置いただけでは文字は入らない。
    /// [`DeclarativeApp::on_focused_input`](crate::DeclarativeApp::on_focused_input)
    /// を実装して欄の状態へ繋ぐ必要がある。 忘れると **フォーカスは入って枠も
    /// 光るのに、 打った文字がどこにも行かない** — コンパイルは通り、 パニックも
    /// せず、 ただ何も起きない。
    ///
    /// ランタイムは実行時に一度 `log::warn!` を出すが、 ログは見落とす。
    /// テストから直接見られるようにしてある:
    ///
    /// ```ignore
    /// h.click("name");
    /// h.text("a");
    /// assert!(h.unrouted_text_inputs().is_empty(), "配線漏れ: {:?}", h.unrouted_text_inputs());
    /// ```
    pub fn unrouted_text_inputs(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self
            .state
            .unrouted_text_inputs()
            .iter()
            .map(|s| s.as_str())
            .collect();
        ids.sort_unstable();
        ids
    }

    /// ペーストされたことにする。 実際のクリップボードには触れない。
    ///
    /// ランタイムの Cmd/Ctrl+V ハンドラは
    /// 「ショートカット判定 → `clipboard::read_text()` → `Paste` を配信」 の 3 段。
    /// ここは最後の配信だけを再現する。 実クリップボードを読む部分は環境依存なので
    /// テストからは外してある (判定部分は `clipboard` モジュールのテストが見ている)。
    pub fn paste(&mut self, text: &str) {
        let ev = sabitori_input::InputEvent::Paste {
            text: text.to_string(),
        };
        crate::runtime_shared::dispatch(
            &mut self.state.app,
            self.state.focused_id.as_deref(),
            &ev,
        );
    }

    /// 管理スクロールコンテナ（`.scroll(id)`）を動かす。 `dy` は logical px、
    /// 正が下方向。 バネの整定を待たずに値を確定させる。
    ///
    /// `id` が管理対象でなければ（`.scroll_manual` や存在しない id）何もしない。
    pub fn scroll(&mut self, id: &str, dy: f32) {
        if let Some(sv) = self.state.scroll_states.get_mut(id) {
            sv.on_scroll_xy(0.0, -dy);
            for _ in 0..240 {
                sv.tick(1.0 / 60.0);
            }
        }
    }

    /// 管理スクロールコンテナの現在位置。 管理対象でなければ `None`。
    pub fn scroll_y(&self, id: &str) -> Option<f32> {
        self.state.scroll_states.get(id).map(|sv| sv.scroll_y.value())
    }

    /// このフレームに居る id を並べる。 assert が落ちたときの手掛かり用。
    pub fn visible_ids(&self) -> Vec<String> {
        self.build()
            .hit_regions
            .iter()
            .filter_map(|r| r.id.clone())
            .collect()
    }
}

/// `Element` をレイアウトだけ通す。 ランタイムを組まずにツリーの寸法を見たいとき用。
pub fn layout(root: &Element, width: f32, height: f32) -> BuildResult {
    sabitori_core::build::build_tree_measured(root, width, height, &StubMeasure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::{div, Dimension::Px};

    #[derive(Default)]
    struct Counter {
        clicks: Vec<String>,
        keys: Vec<Key>,
    }

    impl DeclarativeApp for Counter {
        fn view(&self, _ctx: &sabitori_core::ViewContext) -> Element {
            div().flex_col().children([
                div().id("a").w(Px(100.0)).h(Px(40.0)).on_click(|| {}),
                div().id("b").w(Px(100.0)).h(Px(40.0)).on_click(|| {}),
            ])
        }
        fn on_click(&mut self, id: &str) {
            self.clicks.push(id.to_string());
        }
        fn on_input(&mut self, event: &sabitori_input::InputEvent) -> bool {
            if let sabitori_input::InputEvent::KeyInput { key, pressed: true, .. } = event {
                self.keys.push(*key);
            }
            false
        }
    }

    /// id を指定したクリックが、 その要素の `on_click` を撃つこと。
    #[test]
    fn click_by_id_reaches_the_app() {
        let mut h = Harness::new(Counter::default(), 400.0, 300.0);
        h.frame();
        h.click("b");
        assert_eq!(h.app().clicks, vec!["b".to_string()]);
    }

    /// 座標指定のクリックが、 その座標に居る要素に当たること。
    #[test]
    fn click_at_hits_the_element_under_the_point() {
        let mut h = Harness::new(Counter::default(), 400.0, 300.0);
        h.frame();
        let rect = h.rect_of("a").expect("a が見えていること");
        h.click_at(rect.origin.x + 1.0, rect.origin.y + 1.0);
        assert_eq!(h.app().clicks, vec!["a".to_string()]);
    }

    /// キー入力がアプリまで届くこと。
    #[test]
    fn key_reaches_the_app() {
        let mut h = Harness::new(Counter::default(), 400.0, 300.0);
        h.frame();
        h.key(Key::Enter, Modifiers::default());
        assert_eq!(h.app().keys, vec![Key::Enter]);
    }

    /// 居ない id を叩いたら、 何が居るのかを添えて落ちること。
    /// テストが落ちたときに原因を追えるかどうかは、 足場の使い勝手そのもの。
    #[test]
    #[should_panic(expected = "が見当たらない")]
    fn clicking_a_missing_id_says_what_is_there() {
        let mut h = Harness::new(Counter::default(), 400.0, 300.0);
        h.frame();
        h.click("nope");
    }
}
