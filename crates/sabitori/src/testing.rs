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

use sabitori_core::build::{BuildResult, CaretPos, TextMeasure, TextShape};
use sabitori_core::{Element, Size, TextMetrics, Typography};
use sabitori_input::{InputEvent, Key, Modifiers};

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
        // **折り返しは模さないが、 `\n` は数える。**
        //
        // 数えないと `caret_pos` (論理行を数える) と食い違い、 「キャレットは
        // 8 行目にあるのに箱は 1 行ぶんの高さ」という、 現実には起こり得ない
        // 状態でテストが回ってしまう。 実際それでスクロール追従のテストが
        // 通らなかった。
        let lines = content.split('\n');
        let widest = lines.clone().map(|l| l.chars().count()).max().unwrap_or(0);
        let count = content.split('\n').count().max(1);
        TextMetrics {
            size: Size {
                width: widest as f32 * font_size * 0.5,
                height: count as f32 * font_size,
            },
            baseline: font_size * 0.8,
        }
    }

    fn caret_pos(&self, content: &str, byte_offset: usize, shape: TextShape<'_>) -> CaretPos {
        let (line, before) = stub_line_of(content, byte_offset);
        CaretPos {
            x: before.chars().count() as f32 * shape.font_size * 0.5,
            y: line as f32 * shape.font_size,
            line_height: shape.font_size,
            line,
        }
    }

    fn offset_at(&self, content: &str, point: (f32, f32), shape: TextShape<'_>) -> usize {
        let (x, y) = point;
        let target = ((y / shape.font_size).floor().max(0.0)) as usize;
        let mut acc = 0usize;
        for (i, line) in content.split('\n').enumerate() {
            if i == target {
                let cols = (x / (shape.font_size * 0.5)).round().max(0.0) as usize;
                let take: usize = line.chars().take(cols).map(char::len_utf8).sum();
                return acc + take;
            }
            acc += line.len() + 1;
        }
        content.len()
    }

    fn range_rects(
        &self,
        content: &str,
        range: (usize, usize),
        shape: TextShape<'_>,
    ) -> Vec<sabitori_core::Rect> {
        let (lo, hi) = (range.0.min(range.1), range.0.max(range.1));
        let cw = shape.font_size * 0.5;
        let mut out = Vec::new();
        let mut acc = 0usize;
        for (i, line) in content.split('\n').enumerate() {
            let (ls, le) = (acc, acc + line.len());
            if hi > ls && lo < le.max(ls) {
                let a = lo.clamp(ls, le) - ls;
                let b = hi.clamp(ls, le) - ls;
                let x0 = line[..a].chars().count() as f32 * cw;
                let x1 = line[..b].chars().count() as f32 * cw;
                if x1 > x0 {
                    out.push(sabitori_core::Rect::new(
                        x0,
                        i as f32 * shape.font_size,
                        x1 - x0,
                        shape.font_size,
                    ));
                }
            }
            acc = le + 1;
        }
        out
    }
}

/// `byte_offset` が何番目の論理行の、 行頭から何文字目かを返す。
///
/// **スタブは折り返しを模さない。** `measure` が `max_width` を見ていないので、
/// ここだけ折り返すと「測った高さ」と「キャレットの y」が食い違う。 `\n` で
/// 分かれた論理行だけ数え、 内部で辻褄を合わせておく。 実際の折り返しの検証は
/// `sabitori-text` 側で本物の shaper に対してやること。
fn stub_line_of(content: &str, byte_offset: usize) -> (usize, &str) {
    let n = byte_offset.min(content.len());
    let mut acc = 0usize;
    for (i, line) in content.split('\n').enumerate() {
        let end = acc + line.len();
        if n <= end {
            let mut cut = n - acc;
            while cut > 0 && !line.is_char_boundary(cut) {
                cut -= 1;
            }
            return (i, &line[..cut]);
        }
        acc = end + 1;
    }
    (0, "")
}

/// [`DeclarativeApp`] をヘッドレスで駆動する。
///
/// 窓も wgpu デバイスも作らない。 モジュールの doc も参照。
pub struct Harness<A: DeclarativeApp> {
    /// クレート内のテストからランタイムの内部状態を直接見るために開けてある
    /// (`declarative.rs` の `draw_gate_tests` 等)。 公開 API ではない。
    pub(crate) state: AppState<A>,
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

    /// いまポインタの下にある要素の id。
    ///
    /// `focused_id` と同じ立場で、`UiCapture` からは引けないので直に出す。
    /// アプリ側からは `ViewContext::hovered` で同じ値が見える。
    ///
    /// 「ホバーしても光らない」は 2 つに分かれる — **塗りが弱い**のか、
    /// **追跡が死んでいる**のか。これはその 2 つを分けるための口
    /// ([#49](https://github.com/Mutafika/sabitori/issues/49))。
    /// アプリが `view()` の中で `ctx.hovered` を控えに書き写して読む手もあるが、
    /// それでは確かめているのがランタイムの状態ではなくアプリの控えになる。
    pub fn hovered_id(&self) -> Option<&str> {
        self.state.hovered_id.as_deref()
    }

    /// いま運んでいる物 — `(payload, 掴んだ要素の id)`。運んでいなければ `None`。
    ///
    /// `press_at` だけでは `None` のまま。`DragManager` は 5px の閾値を越えて
    /// 初めて `Pending` から `Active` に上がるので、`move_to` で動かす必要がある
    /// ([#48](https://github.com/Mutafika/sabitori/issues/48))。
    ///
    /// アプリ側から見た `ViewContext::drag` と違い、こちらは `over_drop_zone` を
    /// 含まない — あれはフレームを組むときに hit_regions から引かれるので、
    /// `frame()` を回した後の `ViewContext` で見ること。
    pub fn drag_info(&self) -> Option<(String, Option<String>)> {
        self.state.drag_manager.drag_info()
    }

    /// いま何かを運んでいるか。
    pub fn dragging(&self) -> bool {
        self.state.drag_manager.is_active()
    }

    /// 座標を指定してポインタを動かす。
    ///
    /// 実機の `CursorMoved` と同じ 1 本 (`pointer_moved_to`) を通るので、
    /// ホバーの更新だけでなく `on_pointer_move` / `on_input(PointerMoved)` /
    /// ドラッグの前進 / テキスト選択の伸長まで、実機と同じことが起きる。
    ///
    /// かつてはホバーの更新しかしておらず、`DragManager` を前に進めていなかった。
    /// 5px の閾値を越えられないので、Harness からはドラッグが永遠に成立せず、
    /// `press_at` → `move_to` → `release` と書いても「押しっぱなしで動かしただけ」
    /// になっていた ([#48](https://github.com/Mutafika/sabitori/issues/48))。
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.state.pointer_moved_to(x, y);
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

    /// 右ボタンを座標で押して離す。 `on_input` に
    /// `PointerPressed` / `PointerReleased { button: Some(Right) }` が届き、
    /// 押下が消費されなければ `on_right_click(id, x, y)` が鳴る (空白なら `""`)。
    pub fn right_click_at(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.state.press_secondary();
        self.state.release_secondary();
    }

    /// `id` の要素の中心を右クリックする。 見えていなければ panic
    /// ([`Self::click`] と同じ)。
    pub fn right_click(&mut self, id: &str) {
        let (x, y) = self.center_of(id);
        self.right_click_at(x, y);
    }

    /// 同じ座標を続けて 2 回クリックする。 回数は実時計で数えるので、 2 打目の
    /// `PointerPressed::click_count` は 2、 同じ対象なら `on_double_click` が鳴る。
    pub fn double_click_at(&mut self, x: f32, y: f32) {
        self.click_at(x, y);
        self.click_at(x, y);
    }

    /// `id` の要素の中心をダブルクリックする。
    pub fn double_click(&mut self, id: &str) {
        let (x, y) = self.center_of(id);
        self.double_click_at(x, y);
    }

    /// ホイール 1 イベントを座標へ送る。 経路は実ランタイムと同じ:
    /// `on_input(InputEvent::Wheel)` → その向きに動ける管理コンテナ →
    /// `on_scroll` / `on_scroll_xy`。 符号は winit と同じで、 **負の `dy` が
    /// 下へスクロール** ([`Self::scroll`] とは逆なので注意)。
    ///
    /// 精密入力 (`precise = true`)、 位相は `Moved` として送る。 ばねは整定
    /// させないので、 位置を読むなら [`Self::settle`] を挟む。
    pub fn wheel_at(&mut self, x: f32, y: f32, dx: f32, dy: f32) {
        self.wheel_phase_at(x, y, dx, dy, sabitori_input::WheelPhase::Moved);
    }

    /// 位相つきのホイール。 トラックパッドの 1 ジェスチャ
    /// (`Started` → `Moved`… → `Ended`) を再現するときに使う。 精密入力として送る。
    pub fn wheel_phase_at(
        &mut self,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        phase: sabitori_input::WheelPhase,
    ) {
        self.move_to(x, y);
        self.state.wheel(dx, dy, true, phase);
    }

    /// 刻みホイール (`precise = false`) を 1 ノッチぶん送る。 `lines` は行数で、
    /// ランタイムが [`sabitori_input::LINE_DELTA_PX`] 倍して px に直す。
    pub fn wheel_lines_at(&mut self, x: f32, y: f32, lines_x: f32, lines_y: f32) {
        self.move_to(x, y);
        let k = sabitori_input::LINE_DELTA_PX;
        self.state
            .wheel(lines_x * k, lines_y * k, false, sabitori_input::WheelPhase::Moved);
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

    /// IME の変換中テキストを流す (日本語変換の途中)。
    ///
    /// `cursor` は `text` 内のバイト範囲で、 IME が「いまここを編集中」と伝えて
    /// くるもの。 `None` なら末尾。 この時点では**確定していない** — 表示には
    /// 出るが、 欄の本文 (`text()`) には入らない。
    ///
    /// ```ignore
    /// h.click("name");
    /// h.text("a");
    /// h.ime_preedit("にほん", None);
    /// assert_eq!(h.app().name.text(), "a");                       // 未確定
    /// assert_eq!(h.app().name.display_text_with_preedit(), "aにほん"); // 見えている
    /// h.ime_commit("日本");
    /// assert_eq!(h.app().name.text(), "a日本");
    /// ```
    pub fn ime_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        self.dispatch_focused(InputEvent::ImePreedit { text: text.to_string(), cursor });
    }

    /// IME の確定 (変換を決定した)。 変換中の文字列が本文に入る。
    pub fn ime_commit(&mut self, text: &str) {
        self.dispatch_focused(InputEvent::ImeCommit { text: text.to_string() });
    }

    /// IME が有効になったことにする。
    pub fn ime_enabled(&mut self) {
        self.dispatch_focused(InputEvent::ImeEnabled);
    }

    /// フォーカス経路 → アプリ の順で 1 イベント配る。 ランタイム本体と同じ順序。
    fn dispatch_focused(&mut self, event: InputEvent) {
        if !self.state.route_to_managed(&event) {
            let handled = match self.state.focused_id.clone() {
                Some(id) => self.state.app.on_focused_input(&id, &event),
                None => false,
            };
            if !handled {
                self.state.app.on_input(&event);
            }
        }
    }

    /// ペーストされたことにする。 実際のクリップボードには触れない。
    ///
    /// ランタイムの Cmd/Ctrl+V ハンドラは
    /// 「ショートカット判定 → `clipboard::read_text()` → `Paste` を配信」 の 3 段。
    /// ここは最後の配信だけを再現する。 実クリップボードを読む部分は環境依存なので
    /// テストからは外してある (判定部分は `clipboard` モジュールのテストが見ている)。
    pub fn paste(&mut self, text: &str) {
        self.dispatch_focused(InputEvent::Paste { text: text.to_string() });
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

#[cfg(test)]
mod drag_and_hover_tests {
    //! Harness からドラッグを最後まで運べること ([#48]) と、ホバー追跡を
    //! 外から読めること ([#49])。
    //!
    //! **[#44] がテストの外に居座っていた区間がここ。** `move_to` が
    //! `DragManager` を前に進めていなかったので、`drag_ghost` を書いても
    //! テストから踏む手段が無く、実機で目視するまで壊れていることに気付けなかった。
    //!
    //! [#48]: https://github.com/Mutafika/sabitori/issues/48
    //! [#49]: https://github.com/Mutafika/sabitori/issues/49
    //! [#44]: https://github.com/Mutafika/sabitori/issues/44

    use super::*;
    use crate::ViewContext;
    use sabitori_core::element::*;

    #[derive(Default)]
    struct DragApp {
        /// `on_drop` が届いた記録: (運んだ物, 落とした先)。
        dropped: Vec<(String, String)>,
    }

    impl DeclarativeApp for DragApp {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div().w(Px(400.0)).h(Px(200.0)).flex_row().children([
                div()
                    .id("card")
                    .w(Px(100.0))
                    .h(Px(100.0))
                    .bg(sabitori_core::Color::new(0.2, 0.4, 0.9, 1.0))
                    .draggable("card-1"),
                div()
                    .id("bin")
                    .w(Px(100.0))
                    .h(Px(100.0))
                    .bg(sabitori_core::Color::new(0.9, 0.2, 0.2, 1.0))
                    .droppable(),
            ])
        }

        fn drag_ghost(&self, _ctx: &ViewContext) -> Option<Element> {
            Some(div().w(Px(20.0)).h(Px(20.0)))
        }

        fn on_drop(&mut self, data: &str, target_id: &str) {
            self.dropped.push((data.into(), target_id.into()));
        }
    }

    fn harness() -> Harness<DragApp> {
        let mut h = Harness::new(DragApp::default(), 400.0, 200.0);
        h.frame();
        h
    }

    /// **報告された形そのもの。** press → move → release で `on_drop` が届く。
    #[test]
    fn a_drag_can_be_carried_all_the_way_to_a_drop() {
        let mut h = harness();
        h.press_at(50.0, 50.0);
        h.move_to(150.0, 50.0); // 閾値 5px を大きく越える
        h.frame();
        h.release();

        assert_eq!(
            h.app().dropped,
            vec![("card-1".to_string(), "bin".to_string())],
            "on_drop が届いていない"
        );
    }

    /// ドラッグ中はランタイムが運搬状態を持っている。
    #[test]
    fn the_drag_is_visible_while_carrying() {
        let mut h = harness();
        h.press_at(50.0, 50.0);
        h.move_to(150.0, 50.0);
        assert!(h.dragging(), "運んでいる最中なのに drag が立っていない");
        assert_eq!(
            h.drag_info().map(|(d, _)| d),
            Some("card-1".to_string()),
            "運んでいる物が取れない"
        );
    }

    /// **5px を越えないと始まらない。** 閾値の扱いは実機と同じ。
    #[test]
    fn a_press_without_movement_is_not_a_drag() {
        let mut h = harness();
        h.press_at(50.0, 50.0);
        h.move_to(52.0, 50.0); // 2px — 閾値未満
        assert!(!h.dragging(), "2px で drag が始まってしまった");
        h.release();
        assert!(h.app().dropped.is_empty(), "drop していないのに on_drop が来た");
    }

    /// drop zone の外で離したら、何も落ちない。
    #[test]
    fn releasing_outside_a_drop_zone_drops_nothing() {
        let mut h = harness();
        h.press_at(50.0, 50.0);
        h.move_to(50.0, 180.0); // どちらの箱でもない所
        h.frame();
        h.release();
        assert!(h.app().dropped.is_empty());
    }

    /// #49: ホバー追跡を外から読める。
    #[test]
    fn the_hovered_id_is_readable() {
        let mut h = harness();
        h.move_to(50.0, 50.0);
        assert_eq!(h.hovered_id(), Some("card"));
        h.move_to(150.0, 50.0);
        assert_eq!(h.hovered_id(), Some("bin"));
        h.move_to(50.0, 190.0);
        assert_eq!(h.hovered_id(), None, "どの要素の上でもない");
    }
}
