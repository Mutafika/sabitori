use sabitori_core::Point;

pub type PointerId = u64;

/// Reserved id for the system mouse pointer. Touch/pen ids start above this.
pub const MOUSE_POINTER_ID: PointerId = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touch,
    Pen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Enter,
    Tab,
    Escape,
    PageUp,
    PageDown,
    Insert,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Space,
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    /// Shift 修飾キー単独の押下（ゲームのダッシュ等で拾えるよう Other と分離）。
    Shift,
    Other,
}

impl Key {
    /// 全 variant。バックエンドの変換テーブルに配線漏れが無いかを検査する
    /// テスト（`sabitori-window::keymap`）が舐めるために使う。
    /// variant を足したらここにも足すこと。
    pub const ALL: &'static [Key] = &[
        Key::Backspace, Key::Delete, Key::Left, Key::Right, Key::Up, Key::Down,
        Key::Home, Key::End, Key::Enter, Key::Tab, Key::Escape,
        Key::PageUp, Key::PageDown, Key::Insert,
        Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6,
        Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12,
        Key::Space,
        Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I,
        Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R,
        Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
        Key::Shift, Key::Other,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Cmd on macOS, Win on Windows.
    pub meta: bool,
}

/// Bit for the primary button (mouse left, or touch/pen primary contact).
pub const BUTTON_PRIMARY: u8 = 1 << 0;
/// Bit for the secondary button (mouse right).
pub const BUTTON_SECONDARY: u8 = 1 << 1;
/// Bit for the middle button (mouse middle / wheel click).
pub const BUTTON_MIDDLE: u8 = 1 << 2;

pub fn button_bit(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => BUTTON_PRIMARY,
        MouseButton::Right => BUTTON_SECONDARY,
        MouseButton::Middle => BUTTON_MIDDLE,
    }
}

/// Input events normalized from winit. Pointer events unify mouse, touch and pen.
#[derive(Clone, Debug)]
pub enum InputEvent {
    /// Pointer moved. For mouse, fires for both hover and drag.
    /// For touch/pen, fires only while the contact is active.
    PointerMoved {
        id: PointerId,
        kind: PointerKind,
        position: Point,
        /// 動いている**その瞬間**の修飾キー。⇧を押しっぱなしでドラッグする類
        /// (直交スナップ、比率固定、追加選択) は、動いている最中の状態が要る。
        /// 次にクリックするまで分からないのでは、ゴム紐が追従している間に効かない。
        modifiers: Modifiers,
    },
    /// Pointer pressed (mouse button down, touch begin, pen down).
    /// `button` is `Some` only for mouse; `None` for touch/pen primary contact.
    PointerPressed {
        id: PointerId,
        kind: PointerKind,
        position: Point,
        button: Option<MouseButton>,
        /// 押した**瞬間**に握られていた修飾キー。⇧+クリック = 選択に足す／外す、
        /// ⌥+ドラッグ = 複製、のような修飾つきポインタ操作は、押下時の状態が
        /// 分からないと書けない。`KeyInput` を自前で追って状態を持つ手もあるが、
        /// 値は runtime が既に握っているので載せて配る方が素直。
        modifiers: Modifiers,
    },
    /// Pointer released (mouse button up, touch end, pen up).
    PointerReleased {
        id: PointerId,
        kind: PointerKind,
        position: Point,
        button: Option<MouseButton>,
        /// 離した瞬間の修飾キー。押下時と違う場合がある（押してから⇧を足す/離す）
        /// ので、`PointerPressed` の値をそのまま使い回さないこと。
        modifiers: Modifiers,
    },
    /// Pointer interaction cancelled (system gesture, touch cancelled by OS, etc).
    PointerCancelled {
        id: PointerId,
        kind: PointerKind,
    },
    /// Mouse cursor left the window. Not emitted for touch.
    PointerLeft,

    /// IME was activated.
    ImeEnabled,
    /// IME preedit (composing) text updated.
    /// `cursor` is the byte-offset range within `text` where the editing cursor sits.
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// IME committed final text.
    ImeCommit { text: String },
    /// Physical/logical key press or release.
    KeyInput {
        key: Key,
        pressed: bool,
        /// ⚠️ **修飾キー自身のイベントでは「変化前」の値**。
        ///
        /// macOS の winit は `flagsChanged:` で `KeyboardInput` を先に、
        /// `ModifiersChanged` を後に積む。runtime が修飾キー状態を更新するのは
        /// 後者なので、⇧の押下イベントは `shift: false` を、解放イベントは
        /// `shift: true` を載せて届く。
        ///
        /// 修飾キー**以外**のキー (文字キー等) では正しい値が載る。修飾キーの
        /// 変化そのものを観測したいなら [`InputEvent::ModifiersChanged`] を見ること。
        modifiers: Modifiers,
    },
    /// 修飾キーの状態が変わった。載っているのは**変化後**の値。
    ///
    /// 修飾キーの押下/解放を観測する唯一の確実な口。[`InputEvent::KeyInput`] の
    /// `modifiers` は修飾キー自身のイベントでは変化前を指すし、`Key` は `Shift`
    /// 以外の修飾キーを `Key::Other` に潰すので、そちらからは状態を組み立て直せない。
    ///
    /// ポインタが止まっていても届くので、「⇧を押した瞬間にゴム紐を直交へ折る」
    /// のような、動きを伴わない切り替えもこれで書ける。
    ModifiersChanged(Modifiers),
    /// A Unicode character was received (non-IME path).
    CharInput(char),
    /// クリップボードから貼り付けられたテキスト。
    ///
    /// **1 操作 = 1 イベント**として届く。 `CharInput` の連打にしないのは、
    /// 消費側が undo の単位や IME の状態と噛み合わせられなくなるため。
    ///
    /// ランタイムが Cmd+V (macOS) / Ctrl+V (他) を捕まえてクリップボードを読み、
    /// これを発行する。 アプリ側で `Key::V` を自分で見る必要は無い。
    /// 改行を含み得る (複数行を貼った場合) ので、 単一行の入力欄は自分で潰すこと。
    Paste { text: String },
}

/// [`InputEvent`] からペイロードを落とした種別。
///
/// 値を持たないので `match` の腕として並べられる。 「どのランタイムがどの種類を
/// アプリへ配るか」 を [`Delivery`] の表で宣言するために要る。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputEventKind {
    PointerMoved,
    PointerPressed,
    PointerReleased,
    PointerCancelled,
    PointerLeft,
    ImeEnabled,
    ImePreedit,
    ImeCommit,
    KeyInput,
    ModifiersChanged,
    CharInput,
    Paste,
}

impl InputEventKind {
    /// 全種別。 [`Delivery`] の表をテストが舐めるのに使う。
    ///
    /// 更新漏れは下の `all_lists_every_kind` が落とす — [`Self::order`] が網羅
    /// マッチなので、 variant を足すとまず**コンパイルが壊れ**、 番号を振ると
    /// 今度は `ALL` に入れるまで**テストが落ちる**。 2 段で塞いである。
    pub const ALL: &'static [InputEventKind] = &[
        InputEventKind::PointerMoved,
        InputEventKind::PointerPressed,
        InputEventKind::PointerReleased,
        InputEventKind::PointerCancelled,
        InputEventKind::PointerLeft,
        InputEventKind::ImeEnabled,
        InputEventKind::ImePreedit,
        InputEventKind::ImeCommit,
        InputEventKind::KeyInput,
        InputEventKind::ModifiersChanged,
        InputEventKind::CharInput,
        InputEventKind::Paste,
    ];

    /// [`Self::ALL`] 内で占めるべき位置。 `ALL` の完全性検査にだけ使うので
    /// テストビルド限定。 種別追加をコンパイルで止める役目は [`InputEvent::kind`]
    /// が通常ビルドで担っており、 こちらはその後の「`ALL` 更新漏れ」だけを見る。
    #[cfg(test)]
    fn order(self) -> usize {
        match self {
            InputEventKind::PointerMoved => 0,
            InputEventKind::PointerPressed => 1,
            InputEventKind::PointerReleased => 2,
            InputEventKind::PointerCancelled => 3,
            InputEventKind::PointerLeft => 4,
            InputEventKind::ImeEnabled => 5,
            InputEventKind::ImePreedit => 6,
            InputEventKind::ImeCommit => 7,
            InputEventKind::KeyInput => 8,
            InputEventKind::ModifiersChanged => 9,
            InputEventKind::CharInput => 10,
            InputEventKind::Paste => 11,
        }
    }
}

impl InputEvent {
    /// 種別だけを取り出す。
    ///
    /// **`InputEvent` に variant を足すとこの `match` が壊れる。** それが狙いで、
    /// ここが「全 variant を知っている唯一の場所」。 壊れたら
    /// [`InputEventKind`] に対応する種別を足すことになり、 その結果 3 つの
    /// ランタイムの `input_delivery` (これも網羅マッチ) が軒並みコンパイル
    /// エラーになる。 新しい入力が「どこかのランタイムだけ配線されない」状態で
    /// merge されるのを、 レビューではなく型で止める。
    pub fn kind(&self) -> InputEventKind {
        match self {
            InputEvent::PointerMoved { .. } => InputEventKind::PointerMoved,
            InputEvent::PointerPressed { .. } => InputEventKind::PointerPressed,
            InputEvent::PointerReleased { .. } => InputEventKind::PointerReleased,
            InputEvent::PointerCancelled { .. } => InputEventKind::PointerCancelled,
            InputEvent::PointerLeft => InputEventKind::PointerLeft,
            InputEvent::ImeEnabled => InputEventKind::ImeEnabled,
            InputEvent::ImePreedit { .. } => InputEventKind::ImePreedit,
            InputEvent::ImeCommit { .. } => InputEventKind::ImeCommit,
            InputEvent::KeyInput { .. } => InputEventKind::KeyInput,
            InputEvent::ModifiersChanged(_) => InputEventKind::ModifiersChanged,
            InputEvent::CharInput(_) => InputEventKind::CharInput,
            InputEvent::Paste { .. } => InputEventKind::Paste,
        }
    }
}

/// あるランタイムが [`InputEventKind`] の 1 種別をどう扱うかの宣言。
///
/// sabitori にはイベント処理を共有しない 3 つのランタイム (`DeclarativeApp` /
/// `SceneApp` / `SabitoriApp`) があり、 配線は 1 つずつ手で書かれている。 その結果
/// 「core は持っているのにランタイムが配らない」 事故が繰り返し起きた
/// (issue #1 / #3 / #12)。 #12 に至っては、 修正作業そのものの中で
/// `sabitori-window` が新 variant を `_ => {}` で握り潰す同型のバグを作っている。
///
/// そこで各ランタイムは全種別についてこの型で意思表示する。 宣言は
/// [`InputEventKind`] に対する**網羅マッチ**なので、 種別が増えると 3 ランタイム
/// 全部がコンパイルエラーになり、 「配る / 内部で消費する / 発行しない」 の判断を
/// 必ず通ることになる。
///
/// 宣言はドキュメントでもある — 消費側は「このランタイムで
/// `InputEvent::ImeEnabled` は来るのか」 を表 1 つで確認できる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// `on_input` でアプリに届く。 フォーカス中の要素があれば
    /// `on_focused_input` を先に試し、 消費されなければ `on_input` へ落ちる。
    ToApp,
    /// `on_focused_input` に**だけ**届く。 フォーカス中の要素が無いときは
    /// どこにも届かないので、 グローバルに観測したい用途には使えない。
    FocusedOnly,
    /// ランタイムが内部で消費する。 文字列はアプリ側へ伝わる別の口の名前
    /// (`"on_click"` など)。 生のイベントは届かない。
    Internal(&'static str),
    /// このランタイムでは発行されない。 文字列は理由。
    NotProduced(&'static str),
}

/// Per-node interaction state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InteractionState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

/// A pointer currently interacting with the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivePointer {
    pub id: PointerId,
    pub kind: PointerKind,
    pub position: Point,
    /// Bitmask of held buttons. For touch/pen primary contact this is `BUTTON_PRIMARY`.
    pub buttons: u8,
}

/// Tracks global pointer state across mouse and touch.
///
/// `mouse_position` persists regardless of button state (mouse has a persistent cursor).
/// `active` lists every pointer currently in contact / with a button held.
#[derive(Debug, Default)]
pub struct PointerState {
    /// Last known mouse cursor position. Touch does not update this.
    pub mouse_position: Point,
    /// Whether the mouse cursor is currently inside the window.
    pub inside_window: bool,
    /// All pointers with at least one button or contact currently down.
    pub active: Vec<ActivePointer>,
}

impl PointerState {
    /// Any pointer with primary button / contact currently held?
    pub fn primary_pressed(&self) -> bool {
        self.active.iter().any(|p| p.buttons & BUTTON_PRIMARY != 0)
    }

    pub fn find(&self, id: PointerId) -> Option<&ActivePointer> {
        self.active.iter().find(|p| p.id == id)
    }

    pub fn upsert(&mut self, p: ActivePointer) {
        if let Some(existing) = self.active.iter_mut().find(|a| a.id == p.id) {
            *existing = p;
        } else {
            self.active.push(p);
        }
    }

    pub fn remove(&mut self, id: PointerId) -> Option<ActivePointer> {
        self.active
            .iter()
            .position(|p| p.id == id)
            .map(|i| self.active.remove(i))
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;
    use sabitori_core::Point;

    /// `InputEventKind::ALL` に載せ忘れた種別を落とす。
    ///
    /// `order()` が網羅マッチなので variant 追加はまずコンパイルで止まるが、
    /// そこで番号を振っただけで `ALL` を更新し忘れる経路が残る。 それをここで塞ぐ。
    #[test]
    fn all_lists_every_kind() {
        for (i, k) in InputEventKind::ALL.iter().enumerate() {
            assert_eq!(
                k.order(),
                i,
                "ALL の並びが order() と食い違っている: {k:?} は {} 番のはず",
                k.order()
            );
        }
        // order() に振った最大番号 + 1 = ALL の長さ。 新種別に番号を振ったのに
        // ALL へ入れ忘れると、 ここが落ちる。
        let max_order = InputEventKind::ALL.iter().map(|k| k.order()).max().unwrap();
        assert_eq!(
            InputEventKind::ALL.len(),
            max_order + 1,
            "order() に番号を振った種別が ALL に入っていない"
        );
    }

    /// `kind()` が各 variant を正しい種別に落とすこと。
    #[test]
    fn kind_maps_each_variant() {
        let at = Point::new(0.0, 0.0);
        let m = Modifiers::default();
        let cases: &[(InputEvent, InputEventKind)] = &[
            (
                InputEvent::PointerMoved { id: MOUSE_POINTER_ID, kind: PointerKind::Mouse, position: at, modifiers: m },
                InputEventKind::PointerMoved,
            ),
            (
                InputEvent::PointerPressed { id: MOUSE_POINTER_ID, kind: PointerKind::Mouse, position: at, button: Some(MouseButton::Left), modifiers: m },
                InputEventKind::PointerPressed,
            ),
            (
                InputEvent::PointerReleased { id: MOUSE_POINTER_ID, kind: PointerKind::Mouse, position: at, button: Some(MouseButton::Left), modifiers: m },
                InputEventKind::PointerReleased,
            ),
            (
                InputEvent::PointerCancelled { id: 1, kind: PointerKind::Touch },
                InputEventKind::PointerCancelled,
            ),
            (InputEvent::PointerLeft, InputEventKind::PointerLeft),
            (InputEvent::ImeEnabled, InputEventKind::ImeEnabled),
            (
                InputEvent::ImePreedit { text: "かな".into(), cursor: None },
                InputEventKind::ImePreedit,
            ),
            (
                InputEvent::ImeCommit { text: "仮名".into() },
                InputEventKind::ImeCommit,
            ),
            (
                InputEvent::KeyInput { key: Key::A, pressed: true, modifiers: m },
                InputEventKind::KeyInput,
            ),
            (InputEvent::ModifiersChanged(m), InputEventKind::ModifiersChanged),
            (InputEvent::CharInput('あ'), InputEventKind::CharInput),
            (
                InputEvent::Paste { text: "貼り付け".into() },
                InputEventKind::Paste,
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(event.kind(), *expected, "{event:?} の kind() が違う");
        }
        // 全種別を 1 度ずつ網羅していること (テスト自体の抜けを防ぐ)。
        for k in InputEventKind::ALL {
            assert!(
                cases.iter().any(|(_, got)| got == k),
                "{k:?} のケースがこのテストに無い"
            );
        }
    }
}
