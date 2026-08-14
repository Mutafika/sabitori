use sabitori_core::build::{CaretPos, TextShape};
use sabitori_core::{Color, ViewContext};

/// IME preedit (composing) state.
#[derive(Clone, Debug, Default)]
pub struct PreeditState {
    /// The current composing text (e.g. "にほん" while typing "nihon").
    pub text: String,
    /// Byte-offset cursor range within the preedit text.
    pub cursor: Option<(usize, usize)>,
}

impl PreeditState {
    pub fn is_active(&self) -> bool {
        !self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = None;
    }
}

/// テキスト欄の中身。 [`TextInputState`] 越しにしか触れない。
#[derive(Default)]
pub struct TextInputInner {
    pub text: String,
    pub cursor_pos: usize,
    pub selection_start: Option<usize>,
    pub focused: bool,
    pub placeholder: String,
    /// Current IME preedit (composing) state.
    pub preedit: PreeditState,
    /// キャレット点滅の位相 (0.0..1.0)。 [`Self::tick`] が進める。
    ///
    /// 点滅は以前 `FocusManager` と `TextInput` が別々に持っていて、
    /// `TextInputState` 単体では取れなかった。 [`text_input`] ウィジェットが
    /// 引数を増やさず自己完結するよう、 状態側に寄せてある。
    pub blink: f32,
    /// 欄の左上からキャレットまでの `(x, height)`。 [`text_input`] が描くときに
    /// 実フォントで測って書き込む。 ランタイムが IME 変換候補の位置を出すのに使う
    /// ので、 アプリが `ime_cursor_area` を実装する必要が無くなる。
    pub caret_offset: (f32, f32),
    /// 折り返す複数行の欄か。 [`text_area`] が立てる。
    ///
    /// 立つと Enter が改行になり、 貼り付けが改行を保ち、 ↑↓ と Home/End が
    /// **視覚行**で動くようになる。
    pub multiline: bool,
    /// 直近の [`text_input`] / [`text_area`] が実測したキャレット位置。
    /// 欄の内側 (padding の内側) が原点。
    pub caret: CaretPos,
    /// 「実測してからでないと解けない移動」の予約。
    ///
    /// ↑↓ や Home/End は**折り返し後の行**を知らないと解けないが、 キー入力を
    /// 受ける `on_key` に計測器は無い (計測器が居るのは `view()` の中だけ)。
    /// なのでここに積んでおき、 次の `view()` が実測して解決する。 `view()` は
    /// 入力の直後に必ず回るので、 遅れは表に出ない。
    pub pending: Option<PendingMove>,
    /// ↑↓ を続けている間そのまま保つ x。
    ///
    /// これが無いと、 短い行を 1 度通過した時点で桁が痩せたまま戻らない。
    pub desired_x: Option<f32>,
    /// 直近に実測した欄の内側の幅。 [`text_area`] の折り返し幅。
    ///
    /// レイアウトが終わるまで決まらないので、 **前フレームの値**を使う。
    /// 幅が変わった最初の 1 フレームだけ古い幅で折り返し、 次で追いつく。
    pub measured_width: f32,
}

/// 実測してからでないと解けない移動。 [`TextInputInner::pending`] に積まれる。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PendingMove {
    /// 視覚行 1 つぶん上。
    Up { select: bool },
    /// 視覚行 1 つぶん下。
    Down { select: bool },
    /// 視覚行の先頭。
    LineStart { select: bool },
    /// 視覚行の末尾。
    LineEnd { select: bool },
    /// この座標 (欄の内側が原点) にキャレットを置く。 クリック用。
    ToPoint { x: f32, y: f32, select: bool },
}

impl TextInputInner {
    fn new(placeholder: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            selection_start: None,
            focused: false,
            placeholder: placeholder.into(),
            preedit: PreeditState::default(),
            blink: 0.0,
            caret_offset: (0.0, 0.0),
            multiline: false,
            caret: CaretPos::default(),
            pending: None,
            desired_x: None,
            measured_width: 0.0,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor_pos, s);
        self.cursor_pos += s.len();
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            // Find the previous character boundary
            let prev = self.text[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.drain(prev..self.cursor_pos);
            self.cursor_pos = prev;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor_pos < self.text.len() {
            let next = self.text[self.cursor_pos..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_pos + i)
                .unwrap_or(self.text.len());
            self.text.drain(self.cursor_pos..next);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.text[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            self.cursor_pos = self.text[self.cursor_pos..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_pos + i)
                .unwrap_or(self.text.len());
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_pos = self.text.len();
    }

    pub fn display_text(&self) -> &str {
        if self.text.is_empty() {
            &self.placeholder
        } else {
            &self.text
        }
    }

    pub fn is_placeholder(&self) -> bool {
        self.text.is_empty() && !self.preedit.is_active()
    }

    // ── IME handling ──────────────────────────────────────────────

    /// Called when IME preedit text is updated.
    /// The preedit text is displayed with an underline at the cursor position
    /// but is **not** committed to the buffer yet.
    pub fn on_ime_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        self.preedit.text = text;
        self.preedit.cursor = cursor;
    }

    /// Called when IME commits final text.
    /// Clears preedit state and inserts the committed text into the buffer.
    pub fn on_ime_commit(&mut self, text: &str) {
        self.preedit.clear();
        self.delete_selection();
        self.insert_str(text);
    }

    /// クリップボードから貼り付ける。 選択があれば置き換える。
    ///
    /// **単一行の欄では改行を空白に潰す。** 行の途中で切るより 1 行に均す方が
    /// 壊れ方が素直 (URL やパスを貼る用途ではそもそも改行が入らない)。
    ///
    /// [`Self::multiline`] が立っていれば改行はそのまま入る。 ただし `\r\n` は
    /// `\n` に均す — キャレット計算が「1 バイトの `\n` で行が分かれる」前提な
    /// ので、 `\r` が残ると行末に見えない文字が 1 個ぶら下がる。
    pub fn on_paste(&mut self, text: &str) {
        self.preedit.clear();
        self.delete_selection();
        let normalized: String = if self.multiline {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.chars()
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                .collect()
        };
        self.insert_str(&normalized);
    }

    // ── Keyboard handling ─────────────────────────────────────────

    /// Handle a key event. `modifiers` carries Shift/Ctrl/Alt/Meta state.
    /// Returns `true` if the event was consumed.
    pub fn on_key(&mut self, key: sabitori_input::Key, modifiers: sabitori_input::Modifiers) -> bool {
        use sabitori_input::Key;

        // If IME preedit is active, most keys should be ignored here
        // (they are handled by the platform IME). Only Escape cancels preedit.
        if self.preedit.is_active() {
            if key == Key::Escape {
                self.preedit.clear();
                return true;
            }
            // Let IME handle all other keys during composition
            return false;
        }

        let is_cmd = if cfg!(target_os = "macos") {
            modifiers.meta
        } else {
            modifiers.ctrl
        };

        // 「↑↓ で保つ桁」は縦移動が続いている間だけ有効。 横に動いたり文字を
        // 打ったりしたら捨てる — 残すと、 左右で移動した後の ↑ が元の桁に飛ぶ。
        if !matches!(key, Key::Up | Key::Down) {
            self.desired_x = None;
        }
        // **未解決のクリックは捨てる。**
        //
        // クリックの着地点は実測が要るので次の `view()` まで持ち越されるが、
        // その前にキーが来たら、 キーの方が新しい意思。 残すと「押した直後に
        // 打った文字が入った後で、 カーソルだけ押した場所へ飛ぶ」という順序で
        // 解決されてしまう。
        self.cancel_pending_click();

        match key {
            Key::Backspace => {
                if self.has_selection() {
                    self.delete_selection();
                } else {
                    self.backspace();
                }
                true
            }
            Key::Delete => {
                if self.has_selection() {
                    self.delete_selection();
                } else {
                    self.delete();
                }
                true
            }
            Key::Left => {
                if modifiers.shift {
                    self.extend_selection_left();
                } else {
                    self.collapse_selection();
                    self.move_left();
                }
                true
            }
            Key::Right => {
                if modifiers.shift {
                    self.extend_selection_right();
                } else {
                    self.collapse_selection();
                    self.move_right();
                }
                true
            }
            Key::Home => {
                if self.multiline {
                    // 折り返した行の先頭は実測しないと分からない。
                    self.pending = Some(PendingMove::LineStart { select: modifiers.shift });
                } else if modifiers.shift {
                    self.extend_selection_to(0);
                } else {
                    self.collapse_selection();
                    self.move_home();
                }
                true
            }
            Key::End => {
                if self.multiline {
                    self.pending = Some(PendingMove::LineEnd { select: modifiers.shift });
                } else if modifiers.shift {
                    self.extend_selection_to(self.text.len());
                } else {
                    self.collapse_selection();
                    self.move_end();
                }
                true
            }
            // ↑↓ は**視覚行**で動く。 論理行 (`\n` 区切り) で動かすと、 折り返した
            // 長い段落の中で 1 回押しただけで段落ごと飛ぶ。 実測が要るので予約だけ。
            Key::Up if self.multiline => {
                self.pending = Some(PendingMove::Up { select: modifiers.shift });
                true
            }
            Key::Down if self.multiline => {
                self.pending = Some(PendingMove::Down { select: modifiers.shift });
                true
            }
            Key::A if is_cmd => {
                self.select_all();
                true
            }
            // Cmd+C / Cmd+X / Cmd+V are typically handled at a higher level
            // (clipboard access requires platform APIs). We signal consumption
            // so the app layer knows the intent.
            Key::C if is_cmd => true,
            Key::X if is_cmd => {
                self.delete_selection();
                true
            }
            // ⚠️ Cmd/Ctrl+V は **消費しない** (`false` を返す)。
            //
            // ペーストの実体はランタイムがクリップボードを読んで
            // `InputEvent::Paste` として配る (issue #20)。 ここで `true` を返すと
            // 「このキーは処理済み」 とみなされてランタイムの既定動作 (= まさに
            // そのクリップボード読み) が止まり、 **ペーストが永久に起きない**。
            //
            // 0.4.0 より前はここが `true` を返し、 コメントは「実際のペースト
            // テキストは CharInput か ImeCommit で届く」 と言っていた。 が、
            // クリップボードを読むコードが repo に存在しなかったので何も届かず、
            // 戻り値も誰も読んでいなかったので誰も気づかなかった。
            Key::V if is_cmd => false,
            Key::Z if is_cmd => {
                // Undo is not yet implemented.
                false
            }
            // 複数行の欄では Enter が改行。 単一行では**消費せず**アプリへ流す
            // (検索欄の「決定」やフォーム送信がそこにぶら下がっている)。
            Key::Enter if self.multiline && !is_cmd => {
                self.delete_selection();
                self.insert_char('\n');
                true
            }
            Key::Enter | Key::Tab | Key::Escape => {
                // Bubble up to the app layer.
                false
            }
            _ => false,
        }
    }

    /// Handle a printable character input (non-IME path).
    pub fn on_char(&mut self, ch: char) {
        // Ignore during active IME composition
        if self.preedit.is_active() {
            return;
        }
        self.cancel_pending_click();
        self.desired_x = None;
        self.delete_selection();
        self.insert_char(ch);
    }

    /// 未解決のクリック着地点を捨てる。 [`Self::on_key`] / [`Self::on_char`] が呼ぶ。
    ///
    /// ↑↓ / Home / End の予約は捨てない — あれは今まさに積んだものなので。
    fn cancel_pending_click(&mut self) {
        if matches!(self.pending, Some(PendingMove::ToPoint { .. })) {
            self.pending = None;
        }
    }

    // ── Selection helpers ─────────────────────────────────────────

    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some_and(|s| s != self.cursor_pos)
    }

    /// Returns the ordered (start, end) byte range of the selection.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_start.and_then(|s| {
            if s == self.cursor_pos {
                None
            } else {
                let lo = s.min(self.cursor_pos);
                let hi = s.max(self.cursor_pos);
                Some((lo, hi))
            }
        })
    }

    /// Delete the selected text and collapse the cursor.
    pub fn delete_selection(&mut self) {
        if let Some((lo, hi)) = self.selection_range() {
            self.text.drain(lo..hi);
            self.cursor_pos = lo;
            self.selection_start = None;
        }
    }

    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor_pos = self.text.len();
    }

    fn collapse_selection(&mut self) {
        self.selection_start = None;
    }

    fn extend_selection_left(&mut self) {
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }
        self.move_left();
    }

    fn extend_selection_right(&mut self) {
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }
        self.move_right();
    }

    fn extend_selection_to(&mut self, pos: usize) {
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }
        self.cursor_pos = pos;
    }

    // ── Cursor blink ──────────────────────────────────────────────

    /// 点滅位相を進める。 毎フレーム 1 回呼ぶこと。 フォーカスされていなければ
    /// 何もしない (フォーカスが戻ったとき必ず「見えている」状態から始まる)。
    pub fn tick(&mut self, dt: f32) {
        if self.focused {
            self.blink += dt;
            if self.blink > 1.0 {
                self.blink -= 1.0;
            }
        } else {
            self.blink = 0.0;
        }
    }

    /// いまキャレットを描くべきか。 1 秒周期で前半だけ表示する。
    ///
    /// **変換中は点滅させない。** 未確定文字を編集している最中にキャレットが
    /// 消えると、 どこを編集しているのか分からなくなるため。
    pub fn cursor_visible(&self) -> bool {
        self.focused && (self.preedit.is_active() || self.blink < 0.5)
    }

    /// [`Self::display_text_with_preedit`] の中でキャレットが立つべきバイト位置。
    ///
    /// 変換中は preedit の中の編集位置を指す (IME が `cursor` を教えてくれない
    /// 場合は preedit の末尾)。 プレースホルダ表示中は 0。
    pub fn caret_byte_offset(&self) -> usize {
        if self.preedit.is_active() {
            let within = self
                .preedit
                .cursor
                .map(|(s, _)| s)
                .unwrap_or(self.preedit.text.len());
            self.cursor_pos + within.min(self.preedit.text.len())
        } else if self.text.is_empty() {
            0
        } else {
            self.cursor_pos.min(self.text.len())
        }
    }

    // ── Display helpers ───────────────────────────────────────────

    /// Text to display, including preedit composing text spliced in at cursor.
    pub fn display_text_with_preedit(&self) -> String {
        if self.preedit.is_active() {
            let mut buf = String::with_capacity(self.text.len() + self.preedit.text.len());
            buf.push_str(&self.text[..self.cursor_pos]);
            buf.push_str(&self.preedit.text);
            buf.push_str(&self.text[self.cursor_pos..]);
            buf
        } else if self.text.is_empty() {
            self.placeholder.clone()
        } else {
            self.text.clone()
        }
    }

    /// Byte range within `display_text_with_preedit()` that should be rendered
    /// with an underline to indicate composing text. Returns `None` when no
    /// preedit is active.
    pub fn preedit_underline_range(&self) -> Option<(usize, usize)> {
        if self.preedit.is_active() {
            let start = self.cursor_pos;
            let end = start + self.preedit.text.len();
            Some((start, end))
        } else {
            None
        }
    }

    /// Standard router for a focused text field. Feed it the events the runtime
    /// delivers to `DeclarativeApp::on_focused_input` and it drives editing,
    /// caret movement and IME composition, returning whether the event was
    /// consumed. Lets apps drop the copy-pasted per-field
    /// `match event { CharInput => on_char, KeyInput => on_key, ImePreedit =>
    /// on_ime_preedit, ImeCommit => on_ime_commit, .. }`. Pair with the
    /// [`text_input`] widget for rendering.
    pub fn on_focused_input(&mut self, event: &sabitori_input::InputEvent) -> bool {
        use sabitori_input::InputEvent;
        match event {
            InputEvent::CharInput(ch) => {
                self.on_char(*ch);
                true
            }
            InputEvent::KeyInput { key, pressed: true, modifiers } => {
                self.on_key(*key, *modifiers)
            }
            InputEvent::ImePreedit { text, cursor } => {
                self.on_ime_preedit(text.clone(), *cursor);
                true
            }
            InputEvent::ImeCommit { text } => {
                self.on_ime_commit(text);
                true
            }
            InputEvent::Paste { text } => {
                self.on_paste(text);
                true
            }
            _ => false,
        }
    }
}

/// テキスト欄の状態。 アプリのフィールドに持つ。
///
/// # 配線は要らない
///
/// [`text_input`] を `view()` に置いた時点で、 **ランタイムがこの欄を面倒見ます**。
/// キー入力も IME もペーストもここへ届き、 キャレットの点滅も進み、 フォーカス
/// 状態も反映される。 アプリ側に書くことは何もありません。
///
/// ```ignore
/// struct App { name: TextInputState }
///
/// impl DeclarativeApp for App {
///     fn view(&self, ctx: &ViewContext) -> Element {
///         text_input(ctx, "name", &self.name, &style)   // これで全部
///     }
///     fn on_click(&mut self, id: &str) {
///         if id == "save" { println!("{}", self.name.text()); }
///     }
/// }
/// ```
///
/// # なぜハンドルなのか
///
/// `view(&self)` は不変借用なので、 ランタイムがここへ書き込むには内部可変性が
/// 要る。 0.4.0 より前は代わりにアプリが `on_focused_input` / `tick` /
/// `ime_cursor_area` の 3 つを実装して橋渡ししていたが、 **忘れると
/// フォーカスは入って枠も光るのに打った文字がどこにも行かなかった** —
/// コンパイルは通り、 パニックもせず、 ただ何も起きない。 書き忘れる場所を
/// 無くすために、 状態側を共有ハンドルにした。
///
/// [`Clone`] は中身を複製しない (同じ欄を指す)。 レイアウトの都合で複製が
/// 要る場合も状態は 1 つのまま。
#[derive(Clone, Default)]
pub struct TextInputState(std::rc::Rc<std::cell::RefCell<TextInputInner>>);

impl sabitori_core::Managed for TextInputState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl std::fmt::Debug for TextInputState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputState")
            .field("text", &self.text())
            .field("focused", &self.is_focused())
            .finish()
    }
}

impl TextInputState {
    /// プレースホルダを決めて空の欄を作る。
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self(std::rc::Rc::new(std::cell::RefCell::new(TextInputInner::new(
            placeholder,
        ))))
    }

    /// 初期値を入れて作る。 カーソルは末尾。
    pub fn with_text(placeholder: impl Into<String>, text: impl Into<String>) -> Self {
        let s = Self::new(placeholder);
        s.set_text(text);
        s
    }

    /// 中身を読む。
    pub fn text(&self) -> String {
        self.0.borrow().text.clone()
    }

    /// 中身を差し替える。 カーソルは末尾へ、 変換中があれば捨てる。
    pub fn set_text(&self, text: impl Into<String>) {
        let mut inner = self.0.borrow_mut();
        inner.text = text.into();
        inner.cursor_pos = inner.text.len();
        inner.selection_start = None;
        inner.preedit.clear();
    }

    /// 空にする。
    pub fn clear(&self) {
        self.set_text("");
    }

    /// 空か (プレースホルダ表示中か)。
    pub fn is_empty(&self) -> bool {
        self.0.borrow().text.is_empty()
    }

    /// フォーカスされているか。 **ランタイムが毎フレーム設定する**ので、
    /// アプリから書く必要は無い。
    pub fn is_focused(&self) -> bool {
        self.0.borrow().focused
    }

    /// カーソルのバイト位置。
    pub fn cursor_pos(&self) -> usize {
        self.0.borrow().cursor_pos
    }

    /// いま IME で変換中か。
    pub fn is_composing(&self) -> bool {
        self.0.borrow().preedit.is_active()
    }

    /// プレースホルダ。
    pub fn placeholder(&self) -> String {
        self.0.borrow().placeholder.clone()
    }

    /// プレースホルダを差し替える。
    pub fn set_placeholder(&self, placeholder: impl Into<String>) {
        self.0.borrow_mut().placeholder = placeholder.into();
    }

    /// 中身を借りて読む。 複数の値をまとめて見たいとき用。
    ///
    /// **借りている間に他の `TextInputState` のメソッドを呼ばないこと** —
    /// 同じ欄なら `RefCell` の二重借用でパニックする。
    pub fn with<R>(&self, f: impl FnOnce(&TextInputInner) -> R) -> R {
        f(&self.0.borrow())
    }

    /// 中身を可変で借りる。 標準の編集操作で足りないときの逃げ道。
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut TextInputInner) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }

    // ── ここから下はランタイムが呼ぶ。 アプリから呼ぶ必要は無い ──
    //
    // `#[doc(hidden)] pub` なのは、 ランタイムが別 crate (`sabitori`) に居て
    // `pub(crate)` では届かないから。 doc に出さないことで「アプリ向けの API では
    // ない」 と示している。

    /// 入力イベントを流し込む。 消費したら `true`。
    #[doc(hidden)]
    pub fn handle_input(&self, event: &sabitori_input::InputEvent) -> bool {
        self.0.borrow_mut().on_focused_input(event)
    }

    /// キャレット点滅を進める。
    #[doc(hidden)]
    pub fn advance(&self, dt: f32) {
        self.0.borrow_mut().tick(dt);
    }

    /// フォーカス状態を反映する。 外れたら変換中を捨てる。
    #[doc(hidden)]
    pub fn set_focused(&self, focused: bool) {
        let mut inner = self.0.borrow_mut();
        if inner.focused != focused {
            inner.focused = focused;
            inner.blink = 0.0;
            if !focused {
                inner.preedit.clear();
            }
        }
    }

    /// 表示中の文字列 (変換中を挿し込んだもの)。 描画用。
    pub fn display_text_with_preedit(&self) -> String {
        self.0.borrow().display_text_with_preedit()
    }

    /// キャレットのバイト位置 (変換中なら preedit の中)。 描画用。
    pub fn caret_byte_offset(&self) -> usize {
        self.0.borrow().caret_byte_offset()
    }

    /// キャレットをいま描くべきか。 描画用。
    pub fn cursor_visible(&self) -> bool {
        self.0.borrow().cursor_visible()
    }

    /// プレースホルダ表示中か。 描画用。
    pub fn is_placeholder(&self) -> bool {
        self.0.borrow().is_placeholder()
    }

    /// 変換中の範囲 (表示文字列に対するバイト範囲)。 描画用。
    pub fn preedit_underline_range(&self) -> Option<(usize, usize)> {
        self.0.borrow().preedit_underline_range()
    }

    /// 選択範囲 (バイト)。 描画用。
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.0.borrow().selection_range()
    }

    /// 実測したキャレット位置を記録する ([`text_input`] が呼ぶ)。
    #[doc(hidden)]
    pub fn set_caret_offset(&self, x: f32, height: f32) {
        self.0.borrow_mut().caret_offset = (x, height);
    }

    /// 欄の左上から見たキャレットの `(x, 高さ)`。 ランタイムが IME 候補窓の
    /// 位置決めに使う。
    #[doc(hidden)]
    pub fn caret_offset(&self) -> (f32, f32) {
        self.0.borrow().caret_offset
    }

    /// 直近に実測したキャレット位置を書き込む。 [`text_input`] / [`text_area`]
    /// が描くついでに呼ぶ。
    #[doc(hidden)]
    pub fn set_caret(&self, caret: CaretPos) {
        self.0.borrow_mut().caret = caret;
    }

    /// 直近に実測したキャレット位置。
    pub fn caret(&self) -> CaretPos {
        self.0.borrow().caret
    }

    /// 直近に実測した欄の内側の幅。 [`text_area`] が折り返し幅に使う。
    #[doc(hidden)]
    pub fn measured_width(&self) -> Option<f32> {
        let w = self.0.borrow().measured_width;
        (w > 0.0).then_some(w)
    }

    /// 実測した欄の内側の幅を書き込む。 ランタイムが `on_build` 相当の位置で呼ぶ。
    #[doc(hidden)]
    pub fn set_measured_width(&self, w: f32) {
        self.0.borrow_mut().measured_width = w;
    }

    /// 折り返す複数行の欄にする。 [`text_area`] が呼ぶので、 普通は直に触らない。
    pub fn set_multiline(&self, on: bool) {
        self.0.borrow_mut().multiline = on;
    }

    /// 折り返す複数行の欄か。
    pub fn is_multiline(&self) -> bool {
        self.0.borrow().multiline
    }

    /// この座標 (欄の内側が原点) にキャレットを置くよう予約する。
    /// クリックでカーソルを置くのに、 ランタイムが呼ぶ。
    #[doc(hidden)]
    pub fn request_point(&self, x: f32, y: f32, select: bool) {
        self.0.borrow_mut().pending = Some(PendingMove::ToPoint { x, y, select });
    }

    /// 予約された「実測が要る移動」を解決する。
    ///
    /// **`view()` の中からしか呼べない** — 計測器 (`ctx`) が居るのがそこだけ
    /// だから。 `on_key` は予約を積むだけで、 実際にカーソルが動くのはここ。
    #[doc(hidden)]
    ///
    /// `pad` は欄の padding。 クリック座標は**欄の外枠**が原点で来るので、
    /// 文字が始まる位置に直すのにこれを引く。 引き忘れると、 押した場所より
    /// 常に padding ぶん右下の文字にカーソルが飛ぶ。
    pub fn resolve_pending(
        &self,
        ctx: &ViewContext,
        display: &str,
        shape: TextShape<'_>,
        pad: f32,
    ) {
        let Some(pending) = self.0.borrow_mut().pending.take() else {
            return;
        };
        // 変換中は文字列が preedit を含んでいて、 バイト位置がテキスト本体と
        // 対応しない。 動かすと変換が壊れるので何もしない。
        if self.is_composing() {
            return;
        }

        let cursor = self.0.borrow().cursor_pos;
        let here = ctx.caret_pos(display, cursor, shape);
        let mid = here.line_height * 0.5;

        // ↑↓ は「保っている桁」を優先する。 無ければ今の x から始める。
        let keep_x = self.0.borrow().desired_x.unwrap_or(here.x);

        let (point, select, remember_x) = match pending {
            PendingMove::Up { select } => {
                ((keep_x, here.y - mid), select, true)
            }
            PendingMove::Down { select } => {
                ((keep_x, here.y + here.line_height + mid), select, true)
            }
            // 行の先頭 / 末尾は「うんと左 / うんと右」を突けば `offset_at` が
            // その行の端に丸めてくれる。 行の切れ目を自分で探す必要が無い。
            PendingMove::LineStart { select } => ((-1.0e6, here.y + mid), select, false),
            PendingMove::LineEnd { select } => ((1.0e6, here.y + mid), select, false),
            PendingMove::ToPoint { x, y, select } => ((x - pad, y - pad), select, false),
        };

        let target = ctx.offset_at(display, point, shape);
        let mut inner = self.0.borrow_mut();
        inner.desired_x = if remember_x { Some(keep_x) } else { None };
        if select {
            if inner.selection_start.is_none() {
                inner.selection_start = Some(inner.cursor_pos);
            }
        } else {
            inner.selection_start = None;
        }
        inner.cursor_pos = target.min(inner.text.len());
    }
}

/// Visual style for the [`text_input`] widget.
#[derive(Clone, Copy)]
pub struct TextInputStyle {
    pub bg: Color,
    pub border: Color,
    pub text: Color,
    /// Color used when the field is empty (shows its placeholder).
    pub placeholder: Color,
    pub font_size: f32,
    pub radius: f32,
    pub padding: f32,
    /// フォーカス中の枠線。 未指定なら `border` をそのまま使う。
    pub focus_border: Option<Color>,
    /// キャレットの色。 未指定なら `text` をそのまま使う。
    pub caret: Option<Color>,
    /// 変換中 (IME preedit) の文字に敷く色。 未指定なら塗らない。
    ///
    /// 下線を引くのが一般的だが、 現状 text 要素に下線の表現が無いので背景で示す。
    pub preedit: Option<Color>,
    /// 選択範囲に敷く色。 未指定なら**塗らない**。
    ///
    /// 0.4.0 まではこの色そのものが無く、 選択は state に持っているだけで
    /// 一度も描かれていなかった (Shift+→ で範囲は伸びるのに画面は無反応)。
    pub selection: Option<Color>,
}

impl Default for TextInputStyle {
    fn default() -> Self {
        Self::default_dark()
    }
}

impl TextInputStyle {
    /// 暗色テーマの既定。
    ///
    /// **README が最初からこれを書いていたのに、 関数は存在しなかった。**
    /// `readme_examples.rs` が README を逐語で写さず自前に構築していたので、
    /// 「README のコードが通ること」を見ているつもりで通っていなかった。
    pub fn default_dark() -> Self {
        Self {
            bg: Color::from_hex("#202020"),
            border: Color::from_hex("#404040"),
            text: Color::WHITE,
            placeholder: Color::from_hex("#808080"),
            font_size: 14.0,
            radius: 4.0,
            padding: 8.0,
            focus_border: Some(Color::from_hex("#6c8cff")),
            caret: None,
            preedit: Some(Color::from_hex("#33415e")),
            selection: Some(Color::from_hex("#2d4f7c")),
        }
    }

    /// 明色テーマの既定。
    pub fn default_light() -> Self {
        Self {
            bg: Color::WHITE,
            border: Color::from_hex("#c8c8c8"),
            text: Color::from_hex("#1a1a1a"),
            placeholder: Color::from_hex("#9a9a9a"),
            font_size: 14.0,
            radius: 4.0,
            padding: 8.0,
            focus_border: Some(Color::from_hex("#3b6cd4")),
            caret: None,
            preedit: Some(Color::from_hex("#dbe7ff")),
            selection: Some(Color::from_hex("#b6d3ff")),
        }
    }
}

/// [`TextInputState`] を描く単一行テキスト欄。 **これ 1 本で完結する。**
///
/// - 確定済みテキスト + 変換中 (IME preedit) の文字を合成して表示する
/// - **キャレットを正しい位置に描く** (`ctx` の実フォント計測を使う)
/// - キャレットを点滅させる (変換中は点滅を止める)
/// - 空なら placeholder 色に落とす
///
/// # 使い方
///
/// ```ignore
/// // App のフィールド
/// name: TextInputState,
///
/// // view() — 配線はこれだけ
/// text_input(ctx, "name", &self.name, &TextInputStyle::default_dark())
/// ```
///
/// **アプリ側に書くことは他に何も無い。** キー・IME・ペースト・点滅・
/// フォーカス・変換候補ウィンドウの位置は、 `view()` に置いた時点で
/// ランタイムが面倒を見る (0.4.0 の登録機構)。
///
/// 折り返す複数行の欄は [`text_area`]。
///
/// # 0.4.0 での統合について
///
/// 以前はテキスト欄の実装が 2 つあり、 **どちらも不完全**だった (issue #16):
///
/// | | preedit | キャレット |
/// |---|---|---|
/// | `sabitori_widgets::text_input` | 出る | **描画コードが無かった** |
/// | `sabitori_core::forms::text_input` | 出ない | 描くが**常に文末** |
///
/// 後者が `cursor_pos_px` を受け取って無視していたのは、 呼び出し側に幅を測る手段が
/// 無かったから (issue #15)。 `ViewContext` に計測が通ったので、 ここで 1 本にした。
/// `form_text_input` は削除済み。
pub fn text_input(
    ctx: &sabitori_core::ViewContext,
    id: &str,
    input: &TextInputState,
    style: &TextInputStyle,
) -> sabitori_core::element::Element {
    field(ctx, id, input, style, None, 1.0)
}

/// [`text_input`] と [`text_area`] の実体。
///
/// `wrap_width` が `Some` なら折り返す複数行、 `None` なら単一行。 分岐は
/// この 1 個だけで、 キャレットも選択も IME も同じ経路を通る — 2 本に割ると、
/// 片方だけ直る (そして片方は黙って古いまま) が必ず起きる。
fn field(
    ctx: &sabitori_core::ViewContext,
    id: &str,
    input: &TextInputState,
    style: &TextInputStyle,
    wrap_width: Option<f32>,
    line_height_mult: f32,
) -> sabitori_core::element::Element {
    use sabitori_core::element::{div, text, Dimension::Px};

    // **ここが配線。** ランタイムにこの欄を渡すと、 以後キー・IME・ペーストが
    // 直接ここへ届き、 点滅も進み、 フォーカス状態も反映される。 アプリ側に
    // 書くことは何も無い (0.4.0 より前は 3 メソッドの実装が必要だった)。
    ctx.register_managed(id, std::rc::Rc::new(input.clone()));

    // フォーカスは **今フレームの `ctx`** から取る。
    //
    // ランタイムがこの欄にフォーカス状態を書き込むのは `view()` が終わって
    // 登録リストを受け取った後なので、 保存された値は必ず 1 フレーム古い。
    // それを信じると、 欄を押してからキャレットが出るまで 1 フレーム遅れ、
    // 枠線の色も遅れて変わる。
    input.set_focused(ctx.focused.as_deref() == Some(id));

    let display = input.display_text_with_preedit();
    let showing_placeholder = input.is_placeholder();
    let color = if showing_placeholder { style.placeholder } else { style.text };

    let mut label = text(display.clone())
        .font_size(style.font_size)
        .color(color)
        .line_height(line_height_mult);
    if wrap_width.is_some() {
        // 折り返させるには幅が要る。 中身なりの幅のままだと 1 行に伸び続ける。
        label = label.w_full();
    }

    // 変換中の範囲に色を敷く。 placeholder 表示中は preedit も無い。
    if let (Some(tint), Some((s0, e0))) = (style.preedit, input.preedit_underline_range()) {
        label = label.highlight(sabitori_core::HighlightSpec {
            ranges: vec![(s0, e0)],
            color: tint,
            current: None,
            current_color: tint,
        });
    }

    let mut layers = vec![label];

    // 折り返し幅。 単一行では `None` で、 このとき `caret_pos` は
    // `caret_x` と同じ答えを返す (行が 1 本しか無いので)。
    let shape = TextShape::new(style.font_size).typography(sabitori_core::Typography {
        line_height: Some(line_height_mult),
        ..Default::default()
    });
    let shape = match wrap_width {
        Some(w) => shape.wrap(w),
        None => shape,
    };

    // **実測が要る移動をここで解決する。** ↑↓ / Home / End / クリックは
    // 折り返し後の行を知らないと解けないので、 `on_key` は予約だけ積んで
    // ある。 計測器が居るのはこの `view()` の中だけ。
    input.resolve_pending(ctx, &display, shape, style.padding);

    // キャレット。 表示中の文字列に対する位置を実フォントで測る。 これが
    // できなかったのが issue #15 で、 そのせいで旧実装は文末固定だった。
    let caret = ctx.caret_pos(&display, input.caret_byte_offset(), shape);
    input.set_caret(caret);
    // IME 変換候補の位置決めに使う。 実フォントで測れるのはここだけなので、
    // 描くついでに記録しておく (ランタイムはこれに欄の画面座標を足す)。
    input.set_caret_offset(
        style.padding + caret.x,
        style.font_size * CARET_H_RATIO,
    );

    // 選択範囲。 **文字より下、 キャレットより上**に敷く。
    //
    // 0.4.0 まで選択は state に持っているだけで一度も描かれていなかった —
    // Shift+→ で範囲は伸びるのに画面は何も変わらない、 という状態だった。
    if let (Some(sel_color), Some(range)) = (style.selection, input.selection_range()) {
        for r in ctx.range_rects(&display, range, shape) {
            layers.insert(
                0,
                div()
                    .absolute()
                    .pos(r.origin.x, r.origin.y)
                    .w(Px(r.size.width))
                    .h(Px(r.size.height))
                    .bg(sel_color),
            );
        }
    }

    if input.cursor_visible() {
        layers.push(
            div()
                .absolute()
                .pos(caret.x, caret.y)
                .w(Px(CARET_W))
                .h(Px(caret_height(caret, style.font_size)))
                .bg(style.caret.unwrap_or(style.text)),
        );
    }

    let border = if input.is_focused() {
        style.focus_border.unwrap_or(style.border)
    } else {
        style.border
    };

    div()
        .id(id)
        .focusable()
        // 支援技術から「テキスト入力」として見えるように (issue #21)。
        // 名前は placeholder から取る — 空欄のときに何を入れる欄なのか
        // 分かるのは placeholder だけなので。
        .role(sabitori_core::element::Role::TextInput)
        .label(input.placeholder())
        .w_full()
        .p_px(style.padding)
        .bg(style.bg)
        .border(1.0, border)
        .rounded_px(style.radius)
        // キャレットを絶対配置する基準。 これが無いと祖先まで遡って位置が狂う。
        .position(sabitori_core::element::Position::Relative)
        .child(div().position(sabitori_core::element::Position::Relative).children(layers))
}

/// 折り返す複数行のテキスト欄。 [`text_input`] の複数行版。
///
/// ```ignore
/// // App のフィールド
/// memo: TextInputState,
///
/// // view()
/// text_area(ctx, "memo", &self.memo, &TextInputStyle::default_dark(), 6)
/// ```
///
/// `visible_lines` は欄の高さを行数で指定する。 中身がそれを超えたらスクロール
/// する ([`Element::scroll`] を内側に付けてあるので、 ホイールはランタイムが
/// 面倒を見る)。
///
/// # [`text_input`] との違い
///
/// | | `text_input` | `text_area` |
/// |---|---|---|
/// | Enter | アプリへ流す (フォーム送信など) | **改行を入れる** |
/// | 貼り付け | 改行を空白に潰す | **改行を保つ** (`\r\n` は `\n` に均す) |
/// | ↑ ↓ | アプリへ流す | **視覚行**を 1 つ移動 |
/// | Home / End | 文字列の先頭 / 末尾 | **視覚行**の先頭 / 末尾 |
///
/// ↑↓ が「視覚行」なのが要点。 論理行 (`\n` 区切り) で動かすと、 折り返した
/// 長い段落の中で 1 回押しただけで段落ごと飛ぶ。
///
/// # 配線は要らない
///
/// [`text_input`] と同じで、 `view()` に置いた時点でランタイムがキー・IME・
/// ペースト・点滅・フォーカスを面倒見る。 `Cmd+Enter` は改行にならず
/// アプリへ流れるので、 「送信」をそこに割り当てられる。
pub fn text_area(
    ctx: &sabitori_core::ViewContext,
    id: &str,
    input: &TextInputState,
    style: &TextInputStyle,
    visible_lines: u32,
) -> sabitori_core::element::Element {
    use sabitori_core::element::{div, Dimension::Px};

    input.set_multiline(true);

    // 折り返し幅 = 欄の内側の幅。 `ctx.width` ではなく実際の欄の幅が要るが、
    // それはレイアウトが終わるまで決まらない。 前フレームの実測値を使う —
    // 幅が変わった最初の 1 フレームだけ古い幅で折り返すが、 次で追いつく。
    // (初回は `ctx.width` からの見積もり。)
    let wrap = (input.measured_width().unwrap_or(ctx.width) - style.padding * 2.0).max(1.0);

    let line_h = style.font_size * TEXTAREA_LINE_HEIGHT;
    let inner = field(ctx, id, input, style, Some(wrap), TEXTAREA_LINE_HEIGHT);

    div()
        .id(format!("{id}::viewport"))
        .w_full()
        .h(Px(line_h * visible_lines as f32 + style.padding * 2.0))
        .scroll(format!("{id}::viewport"))
        .child(inner)
}

/// [`text_area`] の行の高さ / font_size。 単一行の欄より広く取る — 折り返した
/// 本文は行間が詰まっていると読めない。
const TEXTAREA_LINE_HEIGHT: f32 = 1.5;

/// キャレットの幅 (logical px)。 高 DPI でも 1px 幅は細すぎて消えるので気持ち太め。
const CARET_W: f32 = 1.5;

/// キャレットの縦棒の高さ。
///
/// 単一行 (`line_height` が実測されていない = 0) では `font_size` から出し、
/// 複数行では**その行の高さ**を使う。 折り返した欄で font_size 基準のままだと、
/// 行間が広いときにキャレットだけ短くて浮いて見える。
fn caret_height(caret: CaretPos, font_size: f32) -> f32 {
    if caret.line_height > 0.0 {
        caret.line_height
    } else {
        font_size * CARET_H_RATIO
    }
}
/// キャレットの高さ / font_size。 行の高さいっぱいだと窮屈なので少し詰める。
const CARET_H_RATIO: f32 = 1.2;


// 0.4.0 で retained 版の `TextInput` (`bounds: Rect` を自前で持ち、 自前の
// blink カウンタを回す) を削除した。 #16 で `text_input(ctx, ..)` +
// [`TextInputState`] に一本化したので、 座標を二重管理する側は使い道が無い。

#[cfg(test)]
mod router_tests {
    use super::*;
    use sabitori_input::InputEvent;

    #[test]
    fn on_focused_input_routes_char_and_ime() {
        let mut s = TextInputInner::new("placeholder");
        // Plain char.
        assert!(s.on_focused_input(&InputEvent::CharInput('a')));
        assert_eq!(s.text, "a");
        // IME preedit shows inline but is not committed to the buffer.
        assert!(s.on_focused_input(&InputEvent::ImePreedit {
            text: "にほん".into(),
            cursor: None,
        }));
        assert_eq!(s.display_text_with_preedit(), "aにほん");
        assert_eq!(s.text, "a");
        // Commit folds the composition in.
        assert!(s.on_focused_input(&InputEvent::ImeCommit { text: "日本".into() }));
        assert_eq!(s.text, "a日本");
    }
}

#[cfg(test)]
mod caret_tests {
    use super::*;

    /// キャレットのバイト位置が、 確定済みテキストのカーソル位置と一致すること。
    #[test]
    fn caret_offset_follows_the_cursor_in_committed_text() {
        let mut s = TextInputInner::new("placeholder");
        s.focused = true;
        for ch in "abcd".chars() {
            s.on_char(ch);
        }
        assert_eq!(s.caret_byte_offset(), 4, "末尾");
        s.move_left();
        s.move_left();
        assert_eq!(s.caret_byte_offset(), 2, "左に 2 文字ぶん");
    }

    /// **issue #16 の要点。** 変換中は、 キャレットが preedit の**中**を指すこと。
    ///
    /// 旧実装はキャレットを text 要素の後ろに並べるだけだったので、 変換中かどうかに
    /// 関わらず常に文末に出ていた。 「いま何を変換しているのか」が分からない。
    #[test]
    fn caret_offset_points_inside_the_preedit() {
        let mut s = TextInputInner::new("placeholder");
        s.focused = true;
        s.on_char('a');
        // 「にほん」を変換中、 IME は 2 文字目まで編集中と伝えてくる (6 バイト)。
        s.on_ime_preedit("にほん".into(), Some((6, 6)));

        assert_eq!(s.display_text_with_preedit(), "aにほん");
        assert_eq!(
            s.caret_byte_offset(),
            1 + 6,
            "確定済み 1 バイト + preedit 内 6 バイト"
        );
        // IME が編集位置を教えない場合は preedit の末尾。
        s.on_ime_preedit("にほん".into(), None);
        assert_eq!(s.caret_byte_offset(), 1 + 9);
    }

    /// 変換中はキャレットを点滅させないこと。 未確定文字を編集している最中に
    /// 消えると、 どこを編集しているか分からなくなる。
    #[test]
    fn caret_does_not_blink_while_composing() {
        let mut s = TextInputInner::new("placeholder");
        s.focused = true;

        // 点滅の「消えている」位相へ進める。
        s.blink = 0.75;
        assert!(!s.cursor_visible(), "通常時は消える位相がある");

        s.on_ime_preedit("にほ".into(), None);
        assert!(s.cursor_visible(), "変換中は常に見えていること");
    }

    /// フォーカスが無ければキャレットは出ないし、 位相も溜まらない。
    #[test]
    fn caret_is_hidden_and_reset_without_focus() {
        let mut s = TextInputInner::new("placeholder");
        s.blink = 0.3;
        s.tick(0.5);
        assert!(!s.cursor_visible());
        assert_eq!(s.blink, 0.0, "非フォーカス中は位相を溜めない");
    }

    /// プレースホルダ表示中はキャレットが先頭に立つこと (末尾ではない)。
    #[test]
    fn caret_sits_at_the_start_while_showing_the_placeholder() {
        let s = TextInputInner::new("名前を入力");
        assert!(s.is_placeholder());
        assert_eq!(s.caret_byte_offset(), 0);
    }
}

#[cfg(test)]
mod paste_tests {
    use super::*;
    use sabitori_input::{InputEvent, Key, Modifiers};

    /// 貼り付けたテキストがカーソル位置に入ること。
    #[test]
    fn paste_inserts_at_the_cursor() {
        let mut s = TextInputInner::new("placeholder");
        for ch in "ab".chars() {
            s.on_char(ch);
        }
        s.move_left();
        assert!(s.on_focused_input(&InputEvent::Paste { text: "XY".into() }));
        assert_eq!(s.text, "aXYb");
        assert_eq!(s.cursor_pos, 3);
    }

    /// 選択があれば置き換えること。
    #[test]
    fn paste_replaces_the_selection() {
        let mut s = TextInputInner::new("placeholder");
        for ch in "abcd".chars() {
            s.on_char(ch);
        }
        s.select_all();
        s.on_focused_input(&InputEvent::Paste { text: "Z".into() });
        assert_eq!(s.text, "Z");
    }

    /// 複数行を貼っても 1 行に均されること。 単一行の欄なので、 行の途中で切るより
    /// 空白に潰す方が壊れ方が素直。
    #[test]
    fn multiline_paste_is_flattened() {
        let mut s = TextInputInner::new("placeholder");
        s.on_focused_input(&InputEvent::Paste { text: "a\r\nb\nc".into() });
        assert_eq!(s.text, "a  b c");
    }

    /// **issue #20 の要点。** Cmd/Ctrl+V は消費してはいけない。
    ///
    /// 消費するとランタイムの既定動作 (= クリップボードを読んで `Paste` を配る)
    /// が止まり、 ペーストが永久に起きない。 0.4.0 より前はここが `true` を
    /// 返していたが、 そもそもクリップボードを読むコードが無く、 戻り値も誰も
    /// 読んでいなかったので誰も気づかなかった。
    #[test]
    fn the_paste_shortcut_is_not_consumed() {
        let mut s = TextInputInner::new("placeholder");
        let primary = if cfg!(target_os = "macos") {
            Modifiers { meta: true, ..Default::default() }
        } else {
            Modifiers { ctrl: true, ..Default::default() }
        };
        assert!(
            !s.on_key(Key::V, primary),
            "消費するとランタイムがクリップボードを読まなくなる"
        );
    }

    /// 変換中に貼ったら、 未確定分は捨ててから入れること。
    #[test]
    fn paste_clears_an_active_preedit() {
        let mut s = TextInputInner::new("placeholder");
        s.on_ime_preedit("にほん".into(), None);
        s.on_focused_input(&InputEvent::Paste { text: "X".into() });
        assert!(!s.preedit.is_active());
        assert_eq!(s.text, "X");
    }
}
