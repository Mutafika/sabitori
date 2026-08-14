use sabitori_core::Color;

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
    /// **改行は空白に潰す。** これは単一行の入力欄なので、 複数行を貼られたときに
    /// 行の途中で切るより、 1 行に均す方が壊れ方が素直 (URL やパスを貼る用途では
    /// そもそも改行が入らない)。
    pub fn on_paste(&mut self, text: &str) {
        self.preedit.clear();
        self.delete_selection();
        let flattened: String = text
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect();
        self.insert_str(&flattened);
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
                if modifiers.shift {
                    self.extend_selection_to(0);
                } else {
                    self.collapse_selection();
                    self.move_home();
                }
                true
            }
            Key::End => {
                if modifiers.shift {
                    self.extend_selection_to(self.text.len());
                } else {
                    self.collapse_selection();
                    self.move_end();
                }
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
        self.delete_selection();
        self.insert_char(ch);
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
/// // view()
/// text_input(ctx, "name", &self.name, &style)
///
/// // DeclarativeApp::on_focused_input
/// fn on_focused_input(&mut self, id: &str, ev: &InputEvent) -> bool {
///     match id {
///         "name" => self.name.on_focused_input(ev),
///         _ => false,
///     }
/// }
///
/// // DeclarativeApp::tick — 点滅を進める
/// fn tick(&mut self, dt: f32) { self.name.tick(dt); }
///
/// // DeclarativeApp::ime_cursor_area — 変換候補ウィンドウの位置
/// //   返さないと候補が画面左上に出る
/// fn ime_cursor_area(&self) -> Option<(f32, f32, f32, f32)> { self.name_caret }
/// ```
///
/// 候補ウィンドウの位置に渡す矩形は [`caret_rect`] で作れる。
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
    use sabitori_core::element::{div, text, Dimension::Px};

    // **ここが配線。** ランタイムにこの欄を渡すと、 以後キー・IME・ペーストが
    // 直接ここへ届き、 点滅も進み、 フォーカス状態も反映される。 アプリ側に
    // 書くことは何も無い (0.4.0 より前は 3 メソッドの実装が必要だった)。
    ctx.register_managed(id, std::rc::Rc::new(input.clone()));

    let display = input.display_text_with_preedit();
    let showing_placeholder = input.is_placeholder();
    let color = if showing_placeholder { style.placeholder } else { style.text };

    let mut label = text(display.clone())
        .font_size(style.font_size)
        .color(color);

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

    // キャレット。 表示中の文字列に対する x を実フォントで測る。 これが
    // できなかったのが issue #15 で、 そのせいで旧実装は文末固定だった。
    let caret_x = ctx.caret_x(&display, input.caret_byte_offset(), style.font_size, false);
    // IME 変換候補の位置決めに使う。 実フォントで測れるのはここだけなので、
    // 描くついでに記録しておく (ランタイムはこれに欄の画面座標を足すだけ)。
    input.set_caret_offset(
        style.padding + caret_x,
        style.font_size * CARET_H_RATIO,
    );

    if input.cursor_visible() {
        let x = caret_x;
        layers.push(
            div()
                .absolute()
                .pos(x, 0.0)
                .w(Px(CARET_W))
                .h(Px(style.font_size * CARET_H_RATIO))
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

/// キャレットの幅 (logical px)。 高 DPI でも 1px 幅は細すぎて消えるので気持ち太め。
const CARET_W: f32 = 1.5;
/// キャレットの高さ / font_size。 行の高さいっぱいだと窮屈なので少し詰める。
const CARET_H_RATIO: f32 = 1.2;

/// `ime_cursor_area` に渡す矩形を組む。
///
/// `field_origin` は [`text_input`] を置いた要素の画面上の左上 (`on_build` で
/// `hit_regions` から引ける)。 変換候補ウィンドウはこの矩形の下に出る。
///
/// これを返さないと winit は候補位置をウィンドウ原点のままにするので、
/// **変換候補が画面の左上に出る**。
pub fn caret_rect(
    ctx: &sabitori_core::ViewContext,
    field_origin: (f32, f32),
    input: &TextInputState,
    style: &TextInputStyle,
) -> (f32, f32, f32, f32) {
    let display = input.display_text_with_preedit();
    let x = ctx.caret_x(&display, input.caret_byte_offset(), style.font_size, false);
    (
        field_origin.0 + style.padding + x,
        field_origin.1 + style.padding,
        CARET_W,
        style.font_size * CARET_H_RATIO,
    )
}

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
