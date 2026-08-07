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
    },
    /// Pointer pressed (mouse button down, touch begin, pen down).
    /// `button` is `Some` only for mouse; `None` for touch/pen primary contact.
    PointerPressed {
        id: PointerId,
        kind: PointerKind,
        position: Point,
        button: Option<MouseButton>,
    },
    /// Pointer released (mouse button up, touch end, pen up).
    PointerReleased {
        id: PointerId,
        kind: PointerKind,
        position: Point,
        button: Option<MouseButton>,
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
        modifiers: Modifiers,
    },
    /// A Unicode character was received (non-IME path).
    CharInput(char),
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
