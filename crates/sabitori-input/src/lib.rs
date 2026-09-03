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

/// ホイール / トラックパッドのジェスチャ位相。winit の `TouchPhase` と同じ 4 値。
///
/// トラックパッドは `Started` → `Moved`… → `Ended` で 1 ジェスチャ (慣性は `Ended`
/// の**後**に `Moved` として続く)。刻みホイールは位相を持たないので常に `Moved`。
/// ランタイムはこれで「1 ジェスチャの間は届け先を固定する」(macOS の latching)
/// を行う。アプリ側でも、慣性の終わりや「指が離れた」の検出に使える。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// 刻みホイール 1 行ぶんを論理ピクセルに直す係数。
///
/// winit の `LineDelta` (マウスのホイール 1 ノッチ = 1 行) は、ランタイムがこれを掛けて
/// `PixelDelta` (トラックパッド) と同じ単位に揃えてから配る。受け手 —
/// [`InputEvent::Wheel`] / `on_scroll_xy` / 管理スクロール — は単位で場合分けしない。
/// 本文 1 行 (≈20px) が 1 ノッチで流れる、というテキスト UI の慣習に合わせた値。
pub const LINE_DELTA_PX: f32 = 20.0;

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
        /// 連続クリックの何回目か (OS の `clickCount` / DOM の `detail` 相当)。
        /// 単独のクリックは `1`、ダブルクリックの 2 打目は `2`、トリプルは `3`。
        ///
        /// winit はこれを配らないので、ランタイムが [`ClickCounter`] で合成する:
        /// **同じボタン**を、前回から [`MULTI_CLICK_INTERVAL`] 以内に、前回の位置から
        /// slop 以内で押したら +1、どれか外れたら `1` に戻る。id 付き要素への
        /// ダブルクリックは `DeclarativeApp::on_double_click` でも受けられるが、
        /// キャンバスのような id の無い面や 3 連打はここで見る。
        click_count: u32,
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

    /// ホイール / トラックパッドのスクロール入力。
    ///
    /// `delta_*` は論理ピクセル。行単位で来る刻みホイール (winit `LineDelta`) は
    /// ランタイムが [`LINE_DELTA_PX`] 倍して揃えるので、受け手は単位で場合分け
    /// しなくてよい。符号は `on_scroll_xy` と同じ: 正の `delta_y` は上の内容を
    /// 見せる向き。
    ///
    /// **管理スクロール (`.scroll(id)`) より先に届く。** `on_input` が `true` を
    /// 返すとそのイベントは消費され、管理コンテナも `on_scroll_xy` も動かない。
    /// Cmd+ホイールでカーソル位置ズーム、のようにアプリが優先で取りたい操作は
    /// ここで書く。`false` を返せば従来どおり: カーソル下の管理コンテナ →
    /// その向きに動けなければ外側のコンテナ → 最後に `on_scroll_xy`。
    Wheel {
        /// カーソル位置 (論理座標)。ズームの軸や、どのペインの上かの判定に使う。
        position: Point,
        delta_x: f32,
        delta_y: f32,
        /// `true` ならピクセル精度の入力 (トラックパッド、精密ホイール)。`false` は
        /// 刻みホイールの行単位を [`LINE_DELTA_PX`] で換算したもの。
        precise: bool,
        /// ジェスチャの位相。刻みホイールは常に [`WheelPhase::Moved`]。
        phase: WheelPhase,
        /// その瞬間の修飾キー。Cmd+ホイール (ズーム) / Shift+ホイール (横) はこれで見る。
        modifiers: Modifiers,
    },

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
    Wheel,
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
        InputEventKind::Wheel,
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
            InputEventKind::Wheel => 5,
            InputEventKind::ImeEnabled => 6,
            InputEventKind::ImePreedit => 7,
            InputEventKind::ImeCommit => 8,
            InputEventKind::KeyInput => 9,
            InputEventKind::ModifiersChanged => 10,
            InputEventKind::CharInput => 11,
            InputEventKind::Paste => 12,
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
            InputEvent::Wheel { .. } => InputEventKind::Wheel,
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

/// 連続クリックとみなす、前回の押下からの最長間隔。
///
/// macOS / Windows の既定 (0.5 秒) に合わせた。短くすると普通の速さのダブルクリックが
/// 取りこぼされ、「効かない」と見える。長くすると遅い 2 打目が誤って数えられるが、
/// そちらの方が苦情になりにくい。
pub const MULTI_CLICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
/// マウス / ペンで、前回の押下位置からこれ以上 (論理 px) 離れたら連続とみなさない。
/// Windows の `SM_CXDOUBLECLK` (4px) と同程度。
pub const MULTI_CLICK_SLOP: f32 = 5.0;
/// タッチの同上。指は同じ場所を狙っても 10px 単位でぶれるので、マウスより緩い。
pub const MULTI_TAP_SLOP: f32 = 24.0;

/// 連続クリックの回数を数える (OS の `clickCount` / DOM の `detail` 相当)。
///
/// winit はクリック回数を配らないので、ランタイムが押下のたびにこれへ通して
/// [`InputEvent::PointerPressed`] の `click_count` に載せる。規則は OS と同じ:
/// **同じボタン**を、前回から [`MULTI_CLICK_INTERVAL`] 以内に、前回の位置から
/// slop ([`MULTI_CLICK_SLOP`] / タッチは [`MULTI_TAP_SLOP`]) 以内で押したら +1、
/// どれか 1 つでも外れたら `1` に戻る。
///
/// 自前の runtime (埋め込みホスト) も同じ規則で数えられるよう公開している。
#[derive(Debug, Default)]
pub struct ClickCounter {
    last: Option<LastPress>,
}

#[derive(Debug)]
struct LastPress {
    at: web_time::Instant,
    position: Point,
    button: Option<MouseButton>,
    count: u32,
}

impl ClickCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 押下を 1 回記録して、その押下の連続回数を返す。
    ///
    /// `now` を外から渡すのはテストのため。実行時は [`Self::press_now`] でよい。
    pub fn press(
        &mut self,
        now: web_time::Instant,
        position: Point,
        button: Option<MouseButton>,
        kind: PointerKind,
    ) -> u32 {
        let slop = match kind {
            PointerKind::Touch => MULTI_TAP_SLOP,
            PointerKind::Mouse | PointerKind::Pen => MULTI_CLICK_SLOP,
        };
        let count = match self.last {
            Some(ref prev)
                if prev.button == button
                    && now.saturating_duration_since(prev.at) <= MULTI_CLICK_INTERVAL
                    && within(prev.position, position, slop) =>
            {
                prev.count + 1
            }
            _ => 1,
        };
        self.last = Some(LastPress { at: now, position, button, count });
        count
    }

    /// [`Self::press`] の `now = Instant::now()` 版。
    pub fn press_now(&mut self, position: Point, button: Option<MouseButton>, kind: PointerKind) -> u32 {
        self.press(web_time::Instant::now(), position, button, kind)
    }

    /// 数え直す。フォーカスを失った、モーダルが開いた、など「前のクリックと
    /// 繋げたくない」境界で呼ぶ。
    pub fn reset(&mut self) {
        self.last = None;
    }
}

fn within(a: Point, b: Point, slop: f32) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy <= slop * slop
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
                InputEvent::PointerPressed { id: MOUSE_POINTER_ID, kind: PointerKind::Mouse, position: at, button: Some(MouseButton::Left), modifiers: m, click_count: 1 },
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
            (
                InputEvent::Wheel { position: at, delta_x: 0.0, delta_y: -20.0, precise: false, phase: WheelPhase::Moved, modifiers: m },
                InputEventKind::Wheel,
            ),
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

#[cfg(test)]
mod click_counter_tests {
    use super::*;
    use std::time::Duration;
    use web_time::Instant;

    fn at(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }
    const LEFT: Option<MouseButton> = Some(MouseButton::Left);

    /// 同じ場所を続けて押せば 1, 2, 3 と増える。
    #[test]
    fn rapid_presses_at_one_spot_count_up() {
        let mut c = ClickCounter::new();
        let t0 = Instant::now();
        assert_eq!(c.press(t0, at(10.0, 10.0), LEFT, PointerKind::Mouse), 1);
        assert_eq!(c.press(t0 + Duration::from_millis(100), at(11.0, 10.0), LEFT, PointerKind::Mouse), 2);
        assert_eq!(c.press(t0 + Duration::from_millis(200), at(10.0, 12.0), LEFT, PointerKind::Mouse), 3);
    }

    /// 間隔は**前回の押下から**測る。3 打目が 1 打目から 0.5 秒を超えていても、
    /// 2 打目から 0.5 秒以内なら続きとして数える (OS の規則)。
    #[test]
    fn interval_is_measured_from_the_previous_press_not_the_first() {
        let mut c = ClickCounter::new();
        let t0 = Instant::now();
        c.press(t0, at(0.0, 0.0), LEFT, PointerKind::Mouse);
        c.press(t0 + Duration::from_millis(400), at(0.0, 0.0), LEFT, PointerKind::Mouse);
        assert_eq!(c.press(t0 + Duration::from_millis(800), at(0.0, 0.0), LEFT, PointerKind::Mouse), 3);
    }

    /// 間隔を超えたら 1 に戻る。境界ちょうどは含む。
    #[test]
    fn a_slow_second_press_starts_over() {
        let mut c = ClickCounter::new();
        let t0 = Instant::now();
        c.press(t0, at(0.0, 0.0), LEFT, PointerKind::Mouse);
        assert_eq!(c.press(t0 + MULTI_CLICK_INTERVAL, at(0.0, 0.0), LEFT, PointerKind::Mouse), 2, "境界は含む");
        let t2 = t0 + MULTI_CLICK_INTERVAL;
        assert_eq!(
            c.press(t2 + MULTI_CLICK_INTERVAL + Duration::from_millis(1), at(0.0, 0.0), LEFT, PointerKind::Mouse),
            1
        );
    }

    /// 離れた場所を押したら 1 に戻る。マウスは 5px、タッチは 24px まで許す。
    #[test]
    fn moving_away_starts_over_with_a_looser_slop_for_touch() {
        let mut c = ClickCounter::new();
        let t0 = Instant::now();
        c.press(t0, at(0.0, 0.0), LEFT, PointerKind::Mouse);
        assert_eq!(c.press(t0, at(6.0, 0.0), LEFT, PointerKind::Mouse), 1, "マウスは 5px を超えたら別のクリック");

        let mut t = ClickCounter::new();
        t.press(t0, at(0.0, 0.0), None, PointerKind::Touch);
        assert_eq!(t.press(t0, at(20.0, 0.0), None, PointerKind::Touch), 2, "タッチは 24px まで同じ場所");
        assert_eq!(t.press(t0, at(50.0, 0.0), None, PointerKind::Touch), 1);
    }

    /// ボタンが変わったら 1 に戻る。左→右→左 は 3 連打ではない。
    #[test]
    fn a_different_button_starts_over() {
        let mut c = ClickCounter::new();
        let t0 = Instant::now();
        c.press(t0, at(0.0, 0.0), LEFT, PointerKind::Mouse);
        assert_eq!(c.press(t0, at(0.0, 0.0), Some(MouseButton::Right), PointerKind::Mouse), 1);
        assert_eq!(c.press(t0, at(0.0, 0.0), LEFT, PointerKind::Mouse), 1);
    }

    /// `reset` の後は必ず 1 から。
    #[test]
    fn reset_forgets_the_previous_press() {
        let mut c = ClickCounter::new();
        let t0 = Instant::now();
        c.press(t0, at(0.0, 0.0), LEFT, PointerKind::Mouse);
        c.reset();
        assert_eq!(c.press(t0, at(0.0, 0.0), LEFT, PointerKind::Mouse), 1);
    }

    /// `press_now` は実時計で数える。連続して呼べば当然 2 になる。
    #[test]
    fn press_now_counts_with_the_wall_clock() {
        let mut c = ClickCounter::new();
        assert_eq!(c.press_now(at(0.0, 0.0), LEFT, PointerKind::Mouse), 1);
        assert_eq!(c.press_now(at(0.0, 0.0), LEFT, PointerKind::Mouse), 2);
    }
}
