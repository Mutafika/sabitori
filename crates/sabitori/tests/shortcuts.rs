//! ショートカットの表 (`Keymap`) と、押して登録する欄 (`shortcut_field`) の通し。
//! 設定画面の「キー」の行をそのまま書いた形: 欄を押して ⌘G を押すと、検索の鍵が ⌘G に
//! なり、待ち受けの間は他のショートカットが鳴らない。

use sabitori::testing::Harness;
use sabitori::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Cmd {
    Find,
    NewTab,
}

impl KeyCommand for Cmd {
    fn id(self) -> &'static str {
        match self {
            Cmd::Find => "find",
            Cmd::NewTab => "new_tab",
        }
    }
}

struct Settings {
    keymap: Keymap<Cmd>,
    find_key: ShortcutFieldState,
    ran: Vec<Cmd>,
    bumped: Vec<Cmd>,
}

impl Settings {
    fn new() -> Self {
        Self {
            keymap: Keymap::new([
                (Cmd::Find, vec![Chord::cmd(Key::F)]),
                (Cmd::NewTab, vec![Chord::cmd(Key::T)]),
            ]),
            find_key: ShortcutFieldState::new(),
            ran: Vec::new(),
            bumped: Vec::new(),
        }
    }
}

impl DeclarativeApp for Settings {
    fn view(&self, ctx: &ViewContext) -> Element {
        div().w_full().h_full().p(Px(20.0)).flex_col().gap(10.0).children([
            shortcut_field(
                ctx,
                "key-find",
                &self.find_key,
                self.keymap.chords(Cmd::Find).first().copied(),
                &ShortcutFieldStyle::from_theme(&ctx.theme),
            ),
            div().id("elsewhere").w(Px(200.0)).h(Px(40.0)),
        ])
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        match self.keymap.lookup_event(event) {
            Some(cmd) => {
                self.ran.push(cmd);
                true
            }
            None => false,
        }
    }

    fn tick(&mut self, _dt: f32) {
        match self.find_key.take_recorded() {
            Some(Recorded::Chord(chord)) => self.bumped = self.keymap.assign(Cmd::Find, Some(chord)),
            Some(Recorded::Cleared) => self.bumped = self.keymap.assign(Cmd::Find, None),
            None => {}
        }
    }
}

const CMD: Modifiers = Modifiers { shift: false, ctrl: false, alt: false, meta: true };

fn press(h: &mut Harness<Settings>, key: Key, mods: Modifiers) {
    h.key(key, mods);
    // Harness::key は修飾キーを押したままにする。次の操作に持ち越さない。
    h.set_modifiers(Modifiers::default());
    // 実機の 1 フレームと同じく、時間を進めて (アプリの `tick` が捕まえた鍵を受け取る) から組む。
    h.tick(0.016);
    h.frame();
}

#[test]
fn shortcuts_run_from_the_keymap() {
    let mut h = Harness::new(Settings::new(), 400.0, 300.0);
    h.frame();
    press(&mut h, Key::F, CMD);
    assert_eq!(h.app().ran, [Cmd::Find]);
}

#[test]
fn recording_a_chord_rebinds_the_command_and_nothing_else_fires_meanwhile() {
    let mut h = Harness::new(Settings::new(), 400.0, 300.0);
    h.frame();
    h.click("key-find");
    h.frame();
    assert!(h.app().find_key.is_armed());

    // 待ち受けの間に ⌘T (新しいタブ) を押しても鳴らない — 欄が捕まえる。
    press(&mut h, Key::T, CMD);
    assert!(h.app().ran.is_empty(), "待ち受けの間は他のショートカットが鳴らない");
    assert_eq!(h.app().keymap.lookup(Chord::cmd(Key::T)), Some(Cmd::Find), "⌘T が検索に付いた");
    assert_eq!(h.app().bumped, [Cmd::NewTab], "新しいタブからは外れたと分かる");
    assert!(!h.app().find_key.is_armed(), "決まったら待ち受けをやめる");

    // 待ち受けが終わったので、⌘T は検索を呼ぶ。
    press(&mut h, Key::T, CMD);
    assert_eq!(h.app().ran, [Cmd::Find]);
}

#[test]
fn backspace_clears_and_escape_or_clicking_away_stops_listening() {
    let mut h = Harness::new(Settings::new(), 400.0, 300.0);
    h.frame();
    h.click("key-find");
    h.frame();
    press(&mut h, Key::Backspace, Modifiers::default());
    assert!(h.app().keymap.chords(Cmd::Find).is_empty(), "⌫ で外れる");

    h.click("key-find");
    h.frame();
    press(&mut h, Key::Escape, Modifiers::default());
    assert!(!h.app().find_key.is_armed(), "Esc でやめる (焦点が外れる)");

    h.click("key-find");
    h.frame();
    h.click("elsewhere");
    h.frame();
    assert!(!h.app().find_key.is_armed(), "他を押したらやめる");
}

/// 登録する鍵を長めに押すと、OS は同じ押下を繰り返し送る。離すまでは欄が吸い、
/// いま割り当てた操作がその場で鳴らない。
#[test]
fn holding_the_recorded_chord_does_not_fire_it() {
    let mut h = Harness::new(Settings::new(), 400.0, 300.0);
    h.frame();
    h.click("key-find");
    h.frame();
    h.key_down(Key::G, CMD);
    h.tick(0.016);
    h.frame();
    assert_eq!(h.app().keymap.lookup(Chord::cmd(Key::G)), Some(Cmd::Find));
    for _ in 0..3 {
        h.key_down(Key::G, CMD);
        h.tick(0.016);
        h.frame();
    }
    assert!(h.app().ran.is_empty(), "押し続けている間は鳴らない");
    h.key_up(Key::G, CMD);
    press(&mut h, Key::G, CMD);
    assert_eq!(h.app().ran, [Cmd::Find], "離してから押せば鳴る");
}
