//! ショートカットを押して登録する欄 (設定画面の「キー」の行)。
//!
//! 押すと待ち受けになり、次に押した鍵の組み合わせ ([`Chord`]) を捕まえる。捕まえた鍵は
//! [`ShortcutFieldState::take_recorded`] で受け取り、アプリが [`Keymap::assign`] などで
//! 表に入れる (割り当ててよい鍵か、どの操作から外れたかを決めるのはアプリ)。
//!
//! - 待ち受けの間、キーは全部この欄が受ける (他のショートカットが鳴らない)
//! - 修飾キーだけ (⇧ だけ) では決まらない。⌘ を押したまま T を押して ⌘T
//! - 修飾キー無しの ⌫ / ⌦ は「外す」([`Recorded::Cleared`])
//! - Esc と Tab は焦点を動かす鍵なので登録できない — 押すと待ち受けをやめる
//!
//! ```ignore
//! // 持つ
//! find_key: ShortcutFieldState,
//!
//! // view
//! shortcut_field(ctx, "key-find", &self.find_key, self.keymap.chords(Cmd::Find).first().copied(),
//!     &ShortcutFieldStyle::from_theme(&ctx.theme))
//!
//! // tick
//! match self.find_key.take_recorded() {
//!     Some(Recorded::Chord(ch)) => { let bumped = self.keymap.assign(Cmd::Find, Some(ch)); /* 知らせる */ }
//!     Some(Recorded::Cleared) => { self.keymap.assign(Cmd::Find, None); }
//!     None => {}
//! }
//! ```
//!
//! [`Keymap::assign`]: sabitori_input::Keymap::assign

use std::cell::RefCell;
use std::rc::Rc;

use sabitori_core::element::{div, text, Cursor, Dimension::Px, Element, Role};
use sabitori_core::{AppTheme, Color, ViewContext};
use sabitori_input::{Chord, InputEvent, Key};

/// 待ち受けで捕まえたもの。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// 押された鍵の組み合わせ。
    Chord(Chord),
    /// 修飾キー無しの ⌫ / ⌦ — 割り当てを外したい。
    Cleared,
}

/// 欄の状態。`Rc` のハンドルなので `clone()` は安い。
#[derive(Clone, Default)]
pub struct ShortcutFieldState(Rc<RefCell<Inner>>);

#[derive(Default)]
struct Inner {
    /// 待ち受けている (押してから、鍵が決まるか焦点が外れるまで)。
    armed: bool,
    /// 捕まえて、まだアプリが受け取っていないもの。
    recorded: Option<Recorded>,
    /// 決めた鍵。**離すまで**、その鍵の押下 (押し続けたときの繰り返し)・出る文字・離しを
    /// 吸う。待ち受けは終わっているが、同じ押下の `j` が下の画面へ漏れると一覧が動き、
    /// 押し続けた ⌘G の繰り返しが漏れると、いま割り当てた操作がその場で鳴る。
    held: Option<Key>,
}

impl std::fmt::Debug for ShortcutFieldState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let i = self.0.borrow();
        f.debug_struct("ShortcutFieldState")
            .field("armed", &i.armed)
            .field("recorded", &i.recorded)
            .finish()
    }
}

impl sabitori_core::Managed for ShortcutFieldState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ShortcutFieldState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 待ち受けているか。
    pub fn is_armed(&self) -> bool {
        self.0.borrow().armed
    }

    /// 待ち受けを始める (欄を押したときと同じ)。
    pub fn arm(&self) {
        self.0.borrow_mut().armed = true;
    }

    /// 待ち受けをやめる。
    pub fn disarm(&self) {
        self.0.borrow_mut().armed = false;
    }

    /// 焦点が外れた: 待ち受けをやめ、離しを待っていた鍵も忘れる (離しはもう届かない)。
    fn blur(&self) {
        let mut i = self.0.borrow_mut();
        i.armed = false;
        i.held = None;
    }

    /// 捕まえたものを受け取る (受け取ると空になる)。`tick` などで見る。
    pub fn take_recorded(&self) -> Option<Recorded> {
        self.0.borrow_mut().recorded.take()
    }

    /// 焦点のあるこの欄への入力 (ランタイムが渡す)。待ち受けの間は全部吸って `true`。
    pub fn handle_input(&self, event: &InputEvent) -> bool {
        let mut i = self.0.borrow_mut();
        if !i.armed {
            // 決めた鍵を離すまでは、その鍵の繰り返し・出る文字・離しを吸う。
            let Some(held) = i.held else { return false };
            return match event {
                InputEvent::KeyInput { key, pressed: true, .. } if *key == held => true,
                InputEvent::KeyInput { key, pressed: false, .. } if *key == held => {
                    i.held = None;
                    true
                }
                // 別の鍵を押した: 前の鍵の離しを取りこぼしていても、ここで終わりにする。
                InputEvent::KeyInput { pressed: true, .. } => {
                    i.held = None;
                    false
                }
                InputEvent::CharInput(_) | InputEvent::ImeCommit { .. } | InputEvent::ImePreedit { .. } => true,
                // 修飾キーの離しなどはアプリへ渡す (押しっぱなしの追跡が狂わないように)。
                _ => false,
            };
        }
        if let InputEvent::KeyInput { key, pressed: true, modifiers } = event {
            let plain = !modifiers.meta && !modifiers.ctrl && !modifiers.alt && !modifiers.shift;
            if plain && matches!(key, Key::Backspace | Key::Delete) {
                i.recorded = Some(Recorded::Cleared);
                i.armed = false;
                i.held = Some(*key);
            } else if let Some(chord) = Chord::from_event(event) {
                i.recorded = Some(Recorded::Chord(chord));
                i.armed = false;
                i.held = Some(chord.key);
            }
        }
        // 文字・IME・離した鍵も吸う (待ち受けの間に下の画面へ漏らさない)。
        true
    }
}

/// [`shortcut_field`] の見た目。
#[derive(Clone, Debug)]
pub struct ShortcutFieldStyle {
    pub bg: Color,
    pub border: Color,
    /// 待ち受けの間の枠。
    pub border_armed: Color,
    pub text: Color,
    /// 待ち受けの案内と、鍵が付いていないときの文字。
    pub placeholder: Color,
    pub radius: f32,
    pub font_size: f32,
    pub min_width: f32,
    /// 待ち受けの間に出す文字。
    pub prompt: String,
    /// 鍵が付いていないときに出す文字。
    pub empty: String,
    /// 文字を等幅で組む。
    pub mono: bool,
}

impl ShortcutFieldStyle {
    /// 色は `theme` から。`ctx.theme` をそのまま渡せる。
    pub fn from_theme(theme: &AppTheme) -> Self {
        Self {
            bg: theme.surface,
            border: theme.border,
            border_armed: theme.primary,
            text: theme.text_primary,
            placeholder: theme.text_secondary,
            radius: 6.0,
            font_size: 13.0,
            min_width: 120.0,
            prompt: "Press keys…".into(),
            empty: "—".into(),
            mono: false,
        }
    }
}

/// ショートカットを押して登録する欄。`current` は今付いている鍵 (表から渡す)。
///
/// 押すと焦点が来て待ち受けになる。焦点が外れたら (Esc・Tab・他を押した) やめる。
pub fn shortcut_field(
    ctx: &ViewContext,
    id: &str,
    state: &ShortcutFieldState,
    current: Option<Chord>,
    style: &ShortcutFieldStyle,
) -> Element {
    // キーはランタイムがここへ回す (焦点のある登録済みの欄が先に受ける)。
    ctx.register_managed(id, Rc::new(state.clone()));
    let focused = ctx.focused.as_deref() == Some(id);
    if !focused {
        state.blur();
    }
    {
        let state = state.clone();
        ctx.register_action(id.to_string(), Rc::new(move |_app: &mut dyn std::any::Any| state.arm()));
    }
    let armed = focused && state.is_armed();
    let (label, color) = match (armed, current) {
        (true, _) => (style.prompt.clone(), style.placeholder),
        (false, Some(chord)) => (chord.label(), style.text),
        (false, None) => (style.empty.clone(), style.placeholder),
    };
    let label_el = text(label).font_size(style.font_size).color(color);
    let label_el = if style.mono { label_el.mono() } else { label_el };
    div()
        .id(id)
        .focusable()
        .role(Role::Button)
        .label(current.map_or_else(|| style.empty.clone(), |c| c.label()))
        .cursor(Cursor::Pointer)
        .no_select()
        .min_w(Px(style.min_width))
        .h(Px(style.font_size + 14.0))
        .px_pad(Px(10.0))
        .flex_row()
        .items_center()
        .justify_center()
        .bg(style.bg)
        .border(if armed { 1.5 } else { 1.0 }, if armed { style.border_armed } else { style.border })
        .rounded_px(style.radius)
        .child(label_el.shrink(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_input::Modifiers;

    fn press(key: Key, mods: Modifiers) -> InputEvent {
        InputEvent::KeyInput { key, pressed: true, modifiers: mods }
    }

    #[test]
    fn captures_the_next_chord_and_stops_listening() {
        let s = ShortcutFieldState::new();
        let cmd = Modifiers { meta: true, ..Default::default() };
        assert!(!s.handle_input(&press(Key::T, cmd)), "押す前は受けない");
        s.arm();
        assert!(s.handle_input(&press(Key::Shift, Modifiers { shift: true, ..Default::default() })));
        assert_eq!(s.take_recorded(), None, "修飾キーだけでは決まらない");
        assert!(s.handle_input(&press(Key::T, cmd)));
        assert_eq!(s.take_recorded(), Some(Recorded::Chord(Chord::cmd(Key::T))));
        assert!(!s.is_armed(), "決まったら待ち受けをやめる");
        assert_eq!(s.take_recorded(), None, "受け取ると空");
    }

    #[test]
    fn bare_backspace_clears_but_cmd_backspace_is_a_chord() {
        let s = ShortcutFieldState::new();
        s.arm();
        s.handle_input(&press(Key::Backspace, Modifiers::default()));
        assert_eq!(s.take_recorded(), Some(Recorded::Cleared));
        s.arm();
        s.handle_input(&press(Key::Backspace, Modifiers { meta: true, ..Default::default() }));
        assert_eq!(s.take_recorded(), Some(Recorded::Chord(Chord::cmd(Key::Backspace))));
    }

    #[test]
    fn typed_characters_are_swallowed_while_listening() {
        let s = ShortcutFieldState::new();
        s.arm();
        assert!(s.handle_input(&InputEvent::CharInput('t')), "下の画面に文字を漏らさない");
    }

    /// 鍵を決めた押下で出る文字と離しも吸い、次の押下からは下の画面へ渡す。
    #[test]
    fn the_capturing_press_s_own_text_does_not_leak() {
        let s = ShortcutFieldState::new();
        s.arm();
        assert!(s.handle_input(&press(Key::J, Modifiers::default())));
        assert!(s.handle_input(&InputEvent::CharInput('j')), "同じ押下の文字は吸う");
        assert!(s.handle_input(&InputEvent::KeyInput { key: Key::J, pressed: false, modifiers: Modifiers::default() }));
        assert!(!s.handle_input(&press(Key::K, Modifiers::default())), "次の押下は渡す");
        assert!(!s.handle_input(&InputEvent::CharInput('k')));
    }

    /// 決めた鍵を押し続けたときの繰り返し (OS が押下として送る) も、離すまで吸う。
    /// 前は次の押下で吸うのをやめていたので、⌘G を長めに押すと、いま割り当てた操作が鳴った。
    #[test]
    fn holding_the_captured_key_does_not_leak_its_repeats() {
        let s = ShortcutFieldState::new();
        let cmd = Modifiers { meta: true, ..Default::default() };
        s.arm();
        assert!(s.handle_input(&press(Key::G, cmd)));
        assert!(s.handle_input(&press(Key::G, cmd)), "繰り返しは吸う");
        assert!(s.handle_input(&press(Key::G, cmd)));
        // ⌘ を先に離す: その離しはアプリへ渡し、G はまだ吸う (文字も)。
        let none = Modifiers::default();
        assert!(!s.handle_input(&InputEvent::KeyInput { key: Key::Other, pressed: false, modifiers: none }));
        assert!(s.handle_input(&press(Key::G, none)));
        assert!(s.handle_input(&InputEvent::CharInput('g')));
        assert!(s.handle_input(&InputEvent::KeyInput { key: Key::G, pressed: false, modifiers: none }));
        assert!(!s.handle_input(&press(Key::G, cmd)), "離したあとの押下は渡す");
    }
}
