//! ショートカット: 鍵 1 つと修飾キー ([`Chord`]) と、操作との表 ([`Keymap`])。
//!
//! 設定には `cmd+shift+t` と書き、画面には `⇧⌘T` (macOS) / `Ctrl+Shift+T` (他) と出す。
//! 表は「既定」と「利用者が変えた分」を分けて持ち、保存するのは変えた分だけ — 既定を
//! 後で変えても、利用者が触っていない操作はそれに付いていく。
//!
//! ```ignore
//! #[derive(Clone, Copy, PartialEq, Eq, Hash)]
//! enum Cmd { NewTab, CloseTab, Find }
//!
//! impl KeyCommand for Cmd {
//!     fn id(self) -> &'static str {
//!         match self { Cmd::NewTab => "new_tab", Cmd::CloseTab => "close_tab", Cmd::Find => "find" }
//!     }
//! }
//!
//! let keymap = Keymap::new([
//!     (Cmd::NewTab, vec![Chord::primary(Key::T)]),
//!     (Cmd::CloseTab, vec![Chord::primary(Key::W)]),
//!     (Cmd::Find, vec![Chord::primary(Key::F)]),
//! ])
//! .reserve([Chord::cmd(Key::Q)])
//! .with_overrides(saved);                       // 設定から読んだ分
//!
//! // on_input
//! if let Some(cmd) = keymap.lookup_event(event) { self.run(cmd); return true; }
//!
//! // 設定画面で変えた
//! let bumped = keymap.assign(Cmd::Find, Some(chord)); // 同じ鍵を持っていた操作から外れる
//! save(keymap.overrides());
//! ```

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

use crate::{InputEvent, Key, Modifiers};

/// 鍵 1 つと、一緒に押す修飾キー。
///
/// 修飾キーは**ちょうど一致**で比べる (⇧⌘T と ⌘T は別)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: Key,
    pub mods: Modifiers,
}

impl Chord {
    pub fn new(key: Key, mods: Modifiers) -> Self {
        Self { key, mods }
    }

    /// 修飾キー無し。
    pub fn key(key: Key) -> Self {
        Self::new(key, Modifiers::default())
    }

    /// ⌘ (macOS) / Win・Super (他) + `key`。
    pub fn cmd(key: Key) -> Self {
        Self::key(key).meta()
    }

    /// その OS の主な修飾キー + `key`。macOS は ⌘、他は Ctrl (⌘T と Ctrl+T)。
    /// 設定では `mod+t` と書ける。
    pub fn primary(key: Key) -> Self {
        if cfg!(target_os = "macos") { Self::cmd(key) } else { Self::key(key).ctrl() }
    }

    pub fn shift(mut self) -> Self {
        self.mods.shift = true;
        self
    }
    pub fn ctrl(mut self) -> Self {
        self.mods.ctrl = true;
        self
    }
    pub fn alt(mut self) -> Self {
        self.mods.alt = true;
        self
    }
    pub fn meta(mut self) -> Self {
        self.mods.meta = true;
        self
    }

    /// 押された鍵から。修飾キーだけの押下 (⇧ だけ等) と離したときは `None`。
    pub fn from_event(event: &InputEvent) -> Option<Self> {
        match event {
            InputEvent::KeyInput { key, pressed: true, modifiers } if !is_modifier_only(*key) => {
                Some(Self::new(*key, *modifiers))
            }
            _ => None,
        }
    }

    /// ⌘ / Ctrl / ⌥ のどれも付いていない (⇧ だけは付いていてもよい) — 文字を打つのと
    /// 同じ押し方。欄に打っている間はショートカットにしない、などの判断に使う。
    pub fn is_plain(self) -> bool {
        !self.mods.meta && !self.mods.ctrl && !self.mods.alt
    }

    /// 設定から読む: `cmd+shift+t` / `Ctrl+Alt+Delete` / `mod+,` / `⇧⌘[`。大文字小文字は
    /// 問わない。修飾キーは `cmd` `command` `meta` `super` `win` `⌘` / `ctrl` `control` `⌃` /
    /// `alt` `opt` `option` `⌥` / `shift` `⇧`、それに OS の主な修飾キーの `mod`。
    /// 読めない名前が 1 つでもあれば `None` (黙って別の鍵にしない)。
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        // 記号の修飾キー (⇧⌘T) は `+` 無しでも続けて書ける。
        let mut mods = Modifiers::default();
        let mut rest = s;
        loop {
            let mut chars = rest.chars();
            match chars.next() {
                Some('⌘') => mods.meta = true,
                Some('⌃') => mods.ctrl = true,
                Some('⌥') => mods.alt = true,
                Some('⇧') => mods.shift = true,
                _ => break,
            }
            rest = chars.as_str().trim_start_matches('+');
        }
        let parts: Vec<&str> = rest.split('+').map(str::trim).collect();
        let (key, names) = parts.split_last()?;
        for name in names {
            match name.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "meta" | "super" | "win" | "⌘" => mods.meta = true,
                "ctrl" | "control" | "⌃" => mods.ctrl = true,
                "alt" | "opt" | "option" | "⌥" => mods.alt = true,
                "shift" | "⇧" => mods.shift = true,
                "mod" | "primary" => {
                    if cfg!(target_os = "macos") {
                        mods.meta = true;
                    } else {
                        mods.ctrl = true;
                    }
                }
                _ => return None,
            }
        }
        Some(Self::new(key_from_name(key)?, mods))
    }

    /// 設定に書く形 (`cmd+ctrl+alt+shift+t`)。修飾キーはいつも同じ並びなので、同じ鍵は
    /// いつも同じ文字列になる。[`Self::parse`] で読み戻せる。
    pub fn to_config(self) -> String {
        let mut s = String::new();
        for (on, name) in [
            (self.mods.meta, "cmd+"),
            (self.mods.ctrl, "ctrl+"),
            (self.mods.alt, "alt+"),
            (self.mods.shift, "shift+"),
        ] {
            if on {
                s.push_str(name);
            }
        }
        s.push_str(key_config_name(self.key));
        s
    }

    /// 画面に出す形。macOS は `⇧⌘T` (記号を ⌃⌥⇧⌘ の順に)、他は `Ctrl+Shift+T`。
    pub fn label(self) -> String {
        if cfg!(target_os = "macos") { self.label_mac() } else { self.label_pc() }
    }

    /// macOS の書き方 (`⌃⌥⇧⌘T`)。
    pub fn label_mac(self) -> String {
        let mut s = String::new();
        for (on, glyph) in [
            (self.mods.ctrl, '⌃'),
            (self.mods.alt, '⌥'),
            (self.mods.shift, '⇧'),
            (self.mods.meta, '⌘'),
        ] {
            if on {
                s.push(glyph);
            }
        }
        s.push_str(match self.key {
            Key::Up => "↑",
            Key::Down => "↓",
            Key::Left => "←",
            Key::Right => "→",
            Key::Enter => "↩",
            Key::Tab => "⇥",
            Key::Backspace => "⌫",
            Key::Delete => "⌦",
            Key::Escape => "⎋",
            Key::PageUp => "⇞",
            Key::PageDown => "⇟",
            Key::Home => "↖",
            Key::End => "↘",
            Key::Space => "Space",
            k => key_display_name(k),
        });
        s
    }

    /// Windows / Linux の書き方 (`Ctrl+Alt+Shift+T`)。
    pub fn label_pc(self) -> String {
        let mut s = String::new();
        for (on, name) in [
            (self.mods.ctrl, "Ctrl+"),
            (self.mods.alt, "Alt+"),
            (self.mods.shift, "Shift+"),
            (self.mods.meta, "Win+"),
        ] {
            if on {
                s.push_str(name);
            }
        }
        s.push_str(match self.key {
            Key::Escape => "Esc",
            Key::Delete => "Del",
            Key::PageUp => "PgUp",
            Key::PageDown => "PgDn",
            Key::Insert => "Ins",
            k => key_display_name(k),
        });
        s
    }
}

impl std::fmt::Display for Chord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// 修飾キーだけの押下 (⇧ や、名前の無い鍵)。単独ではショートカットにならない。
fn is_modifier_only(key: Key) -> bool {
    matches!(key, Key::Shift | Key::Other)
}

/// 鍵と、設定に書く名前・画面に出す名前。設定の名前は小文字で書き、読むときは
/// 大文字小文字を問わない。別名は [`key_from_name`] が受ける。
const KEYS: &[(Key, &str, &str)] = &[
    (Key::A, "a", "A"), (Key::B, "b", "B"), (Key::C, "c", "C"), (Key::D, "d", "D"),
    (Key::E, "e", "E"), (Key::F, "f", "F"), (Key::G, "g", "G"), (Key::H, "h", "H"),
    (Key::I, "i", "I"), (Key::J, "j", "J"), (Key::K, "k", "K"), (Key::L, "l", "L"),
    (Key::M, "m", "M"), (Key::N, "n", "N"), (Key::O, "o", "O"), (Key::P, "p", "P"),
    (Key::Q, "q", "Q"), (Key::R, "r", "R"), (Key::S, "s", "S"), (Key::T, "t", "T"),
    (Key::U, "u", "U"), (Key::V, "v", "V"), (Key::W, "w", "W"), (Key::X, "x", "X"),
    (Key::Y, "y", "Y"), (Key::Z, "z", "Z"),
    (Key::Digit0, "0", "0"), (Key::Digit1, "1", "1"), (Key::Digit2, "2", "2"),
    (Key::Digit3, "3", "3"), (Key::Digit4, "4", "4"), (Key::Digit5, "5", "5"),
    (Key::Digit6, "6", "6"), (Key::Digit7, "7", "7"), (Key::Digit8, "8", "8"),
    (Key::Digit9, "9", "9"),
    (Key::F1, "f1", "F1"), (Key::F2, "f2", "F2"), (Key::F3, "f3", "F3"), (Key::F4, "f4", "F4"),
    (Key::F5, "f5", "F5"), (Key::F6, "f6", "F6"), (Key::F7, "f7", "F7"), (Key::F8, "f8", "F8"),
    (Key::F9, "f9", "F9"), (Key::F10, "f10", "F10"), (Key::F11, "f11", "F11"),
    (Key::F12, "f12", "F12"),
    (Key::Up, "up", "Up"), (Key::Down, "down", "Down"), (Key::Left, "left", "Left"),
    (Key::Right, "right", "Right"), (Key::Home, "home", "Home"), (Key::End, "end", "End"),
    (Key::PageUp, "pageup", "PageUp"), (Key::PageDown, "pagedown", "PageDown"),
    (Key::Enter, "enter", "Enter"), (Key::Tab, "tab", "Tab"), (Key::Escape, "escape", "Escape"),
    (Key::Space, "space", "Space"), (Key::Backspace, "backspace", "Backspace"),
    (Key::Delete, "delete", "Delete"), (Key::Insert, "insert", "Insert"),
    (Key::Period, ".", "."), (Key::Comma, ",", ","), (Key::Slash, "/", "/"),
    (Key::Minus, "-", "-"), (Key::Equal, "=", "="), (Key::BracketLeft, "[", "["),
    (Key::BracketRight, "]", "]"),
];

fn key_config_name(key: Key) -> &'static str {
    KEYS.iter().find(|(k, _, _)| *k == key).map_or("", |(_, name, _)| name)
}

fn key_display_name(key: Key) -> &'static str {
    KEYS.iter().find(|(k, _, _)| *k == key).map_or("?", |(_, _, name)| name)
}

/// 設定の鍵の名前から (大文字小文字は問わない。`esc` `return` `del` `pgup` などの別名も)。
fn key_from_name(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    let alias = match lower.as_str() {
        "esc" => "escape",
        "return" | "↩" => "enter",
        "del" | "⌦" => "delete",
        "⌫" => "backspace",
        "pgup" => "pageup",
        "pgdn" | "pgdown" => "pagedown",
        "ins" => "insert",
        "period" => ".",
        "comma" => ",",
        "slash" => "/",
        "minus" => "-",
        "equal" | "equals" => "=",
        "bracketleft" => "[",
        "bracketright" => "]",
        "↑" => "up",
        "↓" => "down",
        "←" => "left",
        "→" => "right",
        "⇥" => "tab",
        "⎋" => "escape",
        other => other,
    };
    KEYS.iter().find(|(_, n, _)| *n == alias).map(|(k, _, _)| *k)
}

/// ショートカットを付ける操作 (アプリの enum)。`id` は設定に書く名前で、変えると利用者が
/// 変えた鍵が読めなくなる。
pub trait KeyCommand: Copy + Eq + Hash + 'static {
    fn id(self) -> &'static str;
}

/// 操作とショートカットの表。既定と、利用者が変えた分 ([`Self::overrides`]) を持つ。
#[derive(Clone, Debug)]
pub struct Keymap<C: KeyCommand> {
    /// 既定の鍵。並びは操作を足した順 (設定画面の並び)。
    defaults: Vec<(C, Vec<Chord>)>,
    /// 利用者が変えた分 (操作の id → 鍵の並び)。保存するのはこれだけ。
    overrides: BTreeMap<String, Vec<String>>,
    /// 割り当てさせない鍵 (⌘Q など)。
    reserved: Vec<Chord>,
    /// 今効いている鍵 (`defaults` の順)。
    current: Vec<(C, Vec<Chord>)>,
    /// 鍵 → 操作。
    lookup: HashMap<Chord, C>,
}

impl<C: KeyCommand> Keymap<C> {
    /// 既定の表から。同じ鍵が 2 つの操作にあれば先の方に付き、後の方からは外れる
    /// (1 つの鍵は 1 つの操作にだけ付く — 画面に出る鍵と効く鍵がずれない)。
    pub fn new(defaults: impl IntoIterator<Item = (C, Vec<Chord>)>) -> Self {
        let mut keymap = Self {
            defaults: defaults.into_iter().collect(),
            overrides: BTreeMap::new(),
            reserved: Vec::new(),
            current: Vec::new(),
            lookup: HashMap::new(),
        };
        keymap.rebuild();
        keymap
    }

    /// 割り当てさせない鍵 (⌘Q・⌘C / ⌘V など、OS や欄が使う鍵)。既定や設定に書いてあっても効かない。
    pub fn reserve(mut self, chords: impl IntoIterator<Item = Chord>) -> Self {
        self.reserved.extend(chords);
        self.rebuild();
        self
    }

    /// 設定から読んだ、利用者が変えた分 (操作の id → 鍵の並び)。知らない id・読めない鍵・
    /// 割り当てさせない鍵は効かせない (他の行はそのまま効く)。
    pub fn with_overrides(mut self, overrides: BTreeMap<String, Vec<String>>) -> Self {
        self.overrides = overrides;
        self.rebuild();
        self
    }

    /// 利用者が変えた分。既定から変わった操作だけが入っている — これを保存する。
    pub fn overrides(&self) -> &BTreeMap<String, Vec<String>> {
        &self.overrides
    }

    /// 割り当てさせない鍵か ([`Self::reserve`])。
    pub fn is_reserved(&self, chord: Chord) -> bool {
        self.reserved.contains(&chord)
    }

    /// 操作に今付いている鍵 (無ければ空)。
    pub fn chords(&self, command: C) -> &[Chord] {
        self.current.iter().find(|(c, _)| *c == command).map_or(&[], |(_, v)| v.as_slice())
    }

    /// 画面に出す鍵 (1 つ目)。付いていなければ `None`。
    pub fn label(&self, command: C) -> Option<String> {
        self.chords(command).first().map(|ch| ch.label())
    }

    /// 既定のままか。利用者が変えていなくても、既定の鍵を他の操作に取られていれば
    /// `false` ([`Self::reset`] で取り返せる)。
    pub fn is_default(&self, command: C) -> bool {
        if self.overrides.contains_key(command.id()) {
            return false;
        }
        let defaults: Vec<Chord> =
            self.default_chords(command).into_iter().filter(|ch| !self.reserved.contains(ch)).collect();
        self.chords(command) == defaults
    }

    /// 操作と鍵を、既定の表の順に (設定画面に並べる用)。
    pub fn iter(&self) -> impl Iterator<Item = (C, &[Chord])> {
        self.current.iter().map(|(c, v)| (*c, v.as_slice()))
    }

    /// その鍵で呼ぶ操作。
    pub fn lookup(&self, chord: Chord) -> Option<C> {
        self.lookup.get(&chord).copied()
    }

    /// 押された鍵で呼ぶ操作 (`on_input` からそのまま)。
    pub fn lookup_event(&self, event: &InputEvent) -> Option<C> {
        self.lookup(Chord::from_event(event)?)
    }

    /// 操作の鍵を `chord` 1 つにする (`None` なら外す)。同じ鍵を持っていた**他の操作からは
    /// 外し**、外れた操作を返す (「⌘F は検索から外れました」と知らせる用)。割り当てさせない
    /// 鍵なら何もせず空を返す。
    pub fn assign(&mut self, command: C, chord: Option<Chord>) -> Vec<C> {
        if chord.is_some_and(|ch| self.is_reserved(ch)) {
            return Vec::new();
        }
        let bumped = match chord {
            Some(ch) => self.take_from_others(command, &[ch]),
            None => Vec::new(),
        };
        self.set(command, chord.into_iter().collect());
        self.rebuild();
        bumped
    }

    /// 操作を既定の鍵に戻す。既定の鍵を他の操作が使っていたら、そちらからは外し、
    /// 外れた操作を返す。
    pub fn reset(&mut self, command: C) -> Vec<C> {
        self.overrides.remove(command.id());
        let defaults = self.default_chords(command);
        let bumped = self.take_from_others(command, &defaults);
        self.rebuild();
        bumped
    }

    /// 全部を既定に戻す。
    pub fn reset_all(&mut self) {
        self.overrides.clear();
        self.rebuild();
    }

    fn default_chords(&self, command: C) -> Vec<Chord> {
        self.defaults.iter().find(|(c, _)| *c == command).map_or_else(Vec::new, |(_, v)| v.clone())
    }

    /// `chords` を `command` 以外の操作から外す (外れた操作を返す)。
    fn take_from_others(&mut self, command: C, chords: &[Chord]) -> Vec<C> {
        let mut bumped = Vec::new();
        let current = self.current.clone();
        for (other, have) in current {
            if other == command || !have.iter().any(|h| chords.contains(h)) {
                continue;
            }
            let rest: Vec<Chord> = have.into_iter().filter(|h| !chords.contains(h)).collect();
            self.set(other, rest);
            bumped.push(other);
        }
        bumped
    }

    /// 操作の鍵を決める。既定と同じになったら設定から消す (既定の変更に付いていけるように)。
    fn set(&mut self, command: C, chords: Vec<Chord>) {
        if chords == self.default_chords(command) {
            self.overrides.remove(command.id());
        } else {
            self.overrides
                .insert(command.id().to_string(), chords.iter().map(|ch| ch.to_config()).collect());
        }
    }

    /// 今効いている鍵を組み直す。**1 つの鍵は 1 つの操作にだけ付く。** 取り合いに
    /// なったら、利用者が変えた操作が既定のままの操作に勝ち、同じ側どうしなら表で先の
    /// 方が勝つ。負けた方からは外す — 設定画面に同じ鍵が 2 行出て片方しか効かない、を
    /// 作らない (既定を後から足したら、利用者が前に選んだ鍵とぶつかる)。
    fn rebuild(&mut self) {
        self.current = self
            .defaults
            .iter()
            .map(|(command, defaults)| {
                let chords: Vec<Chord> = match self.overrides.get(command.id()) {
                    Some(list) => list.iter().filter_map(|s| Chord::parse(s)).collect(),
                    None => defaults.clone(),
                };
                let chords = chords.into_iter().filter(|ch| !self.reserved.contains(ch)).collect();
                (*command, chords)
            })
            .collect();
        self.lookup.clear();
        for user_set in [true, false] {
            for (command, chords) in &mut self.current {
                if self.overrides.contains_key(command.id()) != user_set {
                    continue;
                }
                chords.retain(|chord| *self.lookup.entry(*chord).or_insert(*command) == *command);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum Cmd {
        NewTab,
        CloseTab,
        Find,
    }

    impl KeyCommand for Cmd {
        fn id(self) -> &'static str {
            match self {
                Cmd::NewTab => "new_tab",
                Cmd::CloseTab => "close_tab",
                Cmd::Find => "find",
            }
        }
    }

    fn keymap() -> Keymap<Cmd> {
        Keymap::new([
            (Cmd::NewTab, vec![Chord::cmd(Key::T)]),
            (Cmd::CloseTab, vec![Chord::cmd(Key::W)]),
            (Cmd::Find, vec![Chord::cmd(Key::F)]),
        ])
    }

    #[test]
    fn chords_round_trip_through_the_config_form() {
        for s in ["cmd+shift+t", "ctrl+alt+delete", "f5", "cmd+,", "shift+pageup", "cmd+ctrl+alt+shift+[", "space"] {
            let chord = Chord::parse(s).unwrap_or_else(|| panic!("{s} が読めない"));
            assert_eq!(chord.to_config(), s);
        }
        assert_eq!(Chord::parse("Cmd+Shift+T"), Chord::parse("cmd+shift+t"), "大文字小文字は問わない");
        assert_eq!(Chord::parse("⇧⌘T"), Some(Chord::cmd(Key::T).shift()), "記号の修飾キーは続けて書ける");
        assert_eq!(Chord::parse("command+esc"), Some(Chord::cmd(Key::Escape)));
        assert_eq!(Chord::parse("hyper+t"), None, "知らない修飾キーは黙って別の鍵にしない");
        assert_eq!(Chord::parse("cmd+"), None);
        assert_eq!(Chord::parse("cmd+nope"), None);
    }

    #[test]
    fn mod_means_the_platform_primary_modifier() {
        assert_eq!(Chord::parse("mod+t"), Some(Chord::primary(Key::T)));
        if cfg!(target_os = "macos") {
            assert_eq!(Chord::primary(Key::T), Chord::cmd(Key::T));
        } else {
            assert_eq!(Chord::primary(Key::T), Chord::key(Key::T).ctrl());
        }
    }

    #[test]
    fn labels_follow_each_platform_s_conventions() {
        let chord = Chord::cmd(Key::T).shift().alt().ctrl();
        assert_eq!(chord.label_mac(), "⌃⌥⇧⌘T");
        assert_eq!(chord.label_pc(), "Ctrl+Alt+Shift+Win+T");
        assert_eq!(Chord::cmd(Key::Backspace).label_mac(), "⌘⌫");
        assert_eq!(Chord::key(Key::PageDown).ctrl().label_pc(), "Ctrl+PgDn");
        assert_eq!(Chord::cmd(Key::Comma).label_mac(), "⌘,");
    }

    #[test]
    fn a_pressed_key_becomes_a_chord_but_a_bare_modifier_does_not() {
        let mods = Modifiers { meta: true, shift: true, ..Default::default() };
        let press = |key| InputEvent::KeyInput { key, pressed: true, modifiers: mods };
        assert_eq!(Chord::from_event(&press(Key::T)), Some(Chord::cmd(Key::T).shift()));
        assert_eq!(Chord::from_event(&press(Key::Shift)), None);
        assert_eq!(
            Chord::from_event(&InputEvent::KeyInput { key: Key::T, pressed: false, modifiers: mods }),
            None,
            "離したときは鳴らない"
        );
    }

    #[test]
    fn lookup_finds_the_command_for_a_chord() {
        let k = keymap();
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::NewTab));
        assert_eq!(k.lookup(Chord::cmd(Key::T).shift()), None, "修飾キーはちょうど一致");
        assert_eq!(k.label(Cmd::Find).as_deref(), Some(Chord::cmd(Key::F).label().as_str()));
    }

    #[test]
    fn assigning_a_taken_chord_moves_it_and_reports_who_lost_it() {
        let mut k = keymap();
        let bumped = k.assign(Cmd::Find, Some(Chord::cmd(Key::T)));
        assert_eq!(bumped, [Cmd::NewTab]);
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::Find));
        assert!(k.chords(Cmd::NewTab).is_empty());
        assert_eq!(k.lookup(Chord::cmd(Key::F)), None, "前の鍵は外れる");
        // 保存するのは変えた分だけ。
        assert_eq!(k.overrides().get("find"), Some(&vec!["cmd+t".to_string()]));
        assert_eq!(k.overrides().get("new_tab"), Some(&Vec::<String>::new()));
        assert!(k.overrides().get("close_tab").is_none());
    }

    #[test]
    fn going_back_to_the_default_drops_the_override() {
        let mut k = keymap();
        k.assign(Cmd::Find, Some(Chord::cmd(Key::G)));
        assert!(!k.is_default(Cmd::Find));
        k.assign(Cmd::Find, Some(Chord::cmd(Key::F)));
        assert!(k.is_default(Cmd::Find), "既定と同じにしたら設定から消える");
        assert!(k.overrides().is_empty());
    }

    #[test]
    fn reset_restores_defaults_and_takes_them_back_from_others() {
        let mut k = keymap();
        k.assign(Cmd::Find, Some(Chord::cmd(Key::T)));
        let bumped = k.reset(Cmd::NewTab);
        assert_eq!(bumped, [Cmd::Find]);
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::NewTab));
        assert!(k.chords(Cmd::Find).is_empty());
        k.reset_all();
        assert!(k.overrides().is_empty());
        assert_eq!(k.lookup(Chord::cmd(Key::F)), Some(Cmd::Find));
    }

    #[test]
    fn saved_overrides_load_and_bad_lines_are_skipped() {
        let mut saved = BTreeMap::new();
        saved.insert("find".to_string(), vec!["cmd+shift+f".to_string(), "cmd+nonsense".to_string()]);
        saved.insert("gone_command".to_string(), vec!["cmd+g".to_string()]);
        let k = keymap().with_overrides(saved);
        assert_eq!(k.chords(Cmd::Find), [Chord::cmd(Key::F).shift()], "読めない鍵だけ落ちる");
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::NewTab), "他の行はそのまま");
    }

    /// 利用者が選んだ鍵が、別の操作の既定とぶつかる (既定を後から足した・設定を手で
    /// 書いた)。利用者が選んだ方に付き、既定の方からは外れる — 前は両方の行に同じ鍵が
    /// 出て、効くのは表で先の方 (利用者が選んでいない方) だった。
    #[test]
    fn a_user_s_chord_wins_over_another_command_s_default() {
        let mut saved = BTreeMap::new();
        saved.insert("find".to_string(), vec!["cmd+t".to_string()]);
        let mut k = keymap().with_overrides(saved);
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::Find));
        assert_eq!(k.chords(Cmd::Find), [Chord::cmd(Key::T)]);
        assert!(k.chords(Cmd::NewTab).is_empty(), "負けた方には出さない");
        assert!(!k.is_default(Cmd::NewTab), "既定の鍵を取られている (戻せる)");
        assert!(k.is_default(Cmd::CloseTab));
        // 戻すと取り返す。
        assert_eq!(k.reset(Cmd::NewTab), [Cmd::Find]);
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::NewTab));
        assert!(k.is_default(Cmd::NewTab));
    }

    /// 既定の表の中で同じ鍵が 2 つの操作にあれば、先の方にだけ付く。
    #[test]
    fn a_chord_shared_by_two_defaults_goes_to_the_first() {
        let k = Keymap::new([
            (Cmd::NewTab, vec![Chord::cmd(Key::T)]),
            (Cmd::Find, vec![Chord::cmd(Key::T), Chord::cmd(Key::F)]),
        ]);
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::NewTab));
        assert_eq!(k.chords(Cmd::Find), [Chord::cmd(Key::F)]);
    }

    #[test]
    fn reserved_chords_never_bind() {
        let mut saved = BTreeMap::new();
        saved.insert("find".to_string(), vec!["cmd+q".to_string()]);
        let mut k = keymap().reserve([Chord::cmd(Key::Q)]).with_overrides(saved);
        assert_eq!(k.lookup(Chord::cmd(Key::Q)), None, "設定に書いてあっても効かない");
        assert!(k.assign(Cmd::NewTab, Some(Chord::cmd(Key::Q))).is_empty());
        assert_eq!(k.lookup(Chord::cmd(Key::Q)), None);
        assert_eq!(k.lookup(Chord::cmd(Key::T)), Some(Cmd::NewTab), "断った割り当ては何も変えない");
    }
}
