//! winit → `sabitori-input` の変換を 1 箇所に集約する。
//!
//! 3 つのランタイム（`sabitori-window::run` / `sabitori::run_declarative` /
//! `sabitori::run_scene`）がこの変換をそれぞれコピペで持っていた。結果:
//!
//! - `Key` enum に `F1`〜`F12` / `PageUp` / `PageDown` / `Insert` を足したのに
//!   配線されたのは 1 ランタイムだけで、残り 2 つでは `Key::Other` に落ちていた。
//! - `CharInput` のゲートが 3 者 3 様になり、`run_scene` は制御文字も Cmd 押下も
//!   素通しして、Backspace の `\x7f` がテキストとして挿入されていた。
//!
//! 追加・変更はここだけ触ればよく、下の `parity` テストが配線漏れを
//! **コンパイルエラー**にする。
//!
//! このクレートは winit と `sabitori-input` の両方に依存する唯一のクレートなので、
//! 変換の置き場所としてここが最下層になる。`sabitori-input` 側には置かない —
//! あちらは意図的に winit 非依存で、その純粋さが iOS / wasm の別経路を成立させている。

use sabitori_input::{Key, Modifiers};
use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key as WinitKey, KeyCode, ModifiersState, NamedKey, PhysicalKey};

/// winit の logical key を [`Key`] へ変換する。
///
/// `None` は「対応する [`Key`] を持たない名前付きキー」を意味する。呼び出し側が
/// 「イベントを出さない」(`sabitori-window::run`) か「`Key::Other` として出す」
/// (`run_declarative` / `run_scene`) かを選ぶ。文字キーは未知でも
/// `Some(Key::Other)` を返す（修飾キー単独押下と区別するため）。
pub fn key_from_winit(logical: &WinitKey) -> Option<Key> {
    match logical {
        WinitKey::Named(named) => named_key(named),
        WinitKey::Character(c) => Some(character_key(c)),
        _ => None,
    }
}

/// [`key_from_winit`] に、物理キーでの補いを足したもの。ランタイムはこちらを使う。
///
/// 文字で決まらないとき (⇧ で `!` になった数字、US 以外の配列で記号の位置が
/// 違うとき) は、物理キーの位置で数字・記号を当てる。⌘1 や ⇧⌘. のような
/// ショートカットが配列や ⇧ で取りこぼされないように。
pub fn key_from_event(event: &KeyEvent) -> Option<Key> {
    let key = key_from_winit(&event.logical_key)?;
    if key != Key::Other {
        return Some(key);
    }
    Some(match event.physical_key {
        PhysicalKey::Code(code) => physical_key(code).unwrap_or(Key::Other),
        PhysicalKey::Unidentified(_) => Key::Other,
    })
}

fn physical_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Digit0 => Key::Digit0,
        KeyCode::Digit1 => Key::Digit1,
        KeyCode::Digit2 => Key::Digit2,
        KeyCode::Digit3 => Key::Digit3,
        KeyCode::Digit4 => Key::Digit4,
        KeyCode::Digit5 => Key::Digit5,
        KeyCode::Digit6 => Key::Digit6,
        KeyCode::Digit7 => Key::Digit7,
        KeyCode::Digit8 => Key::Digit8,
        KeyCode::Digit9 => Key::Digit9,
        KeyCode::Period => Key::Period,
        KeyCode::Comma => Key::Comma,
        KeyCode::Slash => Key::Slash,
        KeyCode::Minus => Key::Minus,
        KeyCode::Equal => Key::Equal,
        KeyCode::BracketLeft => Key::BracketLeft,
        KeyCode::BracketRight => Key::BracketRight,
        _ => return None,
    })
}

fn named_key(named: &NamedKey) -> Option<Key> {
    Some(match named {
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::Insert => Key::Insert,
        NamedKey::Enter => Key::Enter,
        NamedKey::Tab => Key::Tab,
        NamedKey::Escape => Key::Escape,
        NamedKey::Space => Key::Space,
        NamedKey::F1 => Key::F1,
        NamedKey::F2 => Key::F2,
        NamedKey::F3 => Key::F3,
        NamedKey::F4 => Key::F4,
        NamedKey::F5 => Key::F5,
        NamedKey::F6 => Key::F6,
        NamedKey::F7 => Key::F7,
        NamedKey::F8 => Key::F8,
        NamedKey::F9 => Key::F9,
        NamedKey::F10 => Key::F10,
        NamedKey::F11 => Key::F11,
        NamedKey::F12 => Key::F12,
        // Shift 単独でもイベントを出す（ゲームのダッシュ等で押下を拾うため）。
        NamedKey::Shift => Key::Shift,
        _ => return None,
    })
}

fn character_key(c: &str) -> Key {
    match c.to_ascii_lowercase().as_str() {
        " " => Key::Space,
        "a" => Key::A,
        "b" => Key::B,
        "c" => Key::C,
        "d" => Key::D,
        "e" => Key::E,
        "f" => Key::F,
        "g" => Key::G,
        "h" => Key::H,
        "i" => Key::I,
        "j" => Key::J,
        "k" => Key::K,
        "l" => Key::L,
        "m" => Key::M,
        "n" => Key::N,
        "o" => Key::O,
        "p" => Key::P,
        "q" => Key::Q,
        "r" => Key::R,
        "s" => Key::S,
        "t" => Key::T,
        "u" => Key::U,
        "v" => Key::V,
        "w" => Key::W,
        "x" => Key::X,
        "y" => Key::Y,
        "z" => Key::Z,
        "0" => Key::Digit0,
        "1" => Key::Digit1,
        "2" => Key::Digit2,
        "3" => Key::Digit3,
        "4" => Key::Digit4,
        "5" => Key::Digit5,
        "6" => Key::Digit6,
        "7" => Key::Digit7,
        "8" => Key::Digit8,
        "9" => Key::Digit9,
        // ⇧ 付きの形 (US 配列) も同じキー。他の配列は物理キーで補う
        "." | ">" => Key::Period,
        "," | "<" => Key::Comma,
        "/" | "?" => Key::Slash,
        "-" | "_" => Key::Minus,
        "=" | "+" => Key::Equal,
        "[" | "{" => Key::BracketLeft,
        "]" | "}" => Key::BracketRight,
        _ => Key::Other,
    }
}

/// winit の [`ModifiersState`] を [`Modifiers`] へ。
pub fn modifiers_from_winit(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
    }
}

/// この `KeyEvent` が `CharInput` として送るべき文字を返す。
///
/// テキスト入力の判定方針をここに集約する:
///
/// - **押下時のみ。** 離鍵でテキストは入らない。
/// - **`event.text` を使う。** logical key の文字ではなくプラットフォームが
///   決めたテキストを使う。dead key や Option+文字の合成（`é`, `©`）が正しく通る。
/// - **制御文字は落とす。** macOS の Backspace は `"\x7f"`、Enter は `"\r"`、
///   Tab は `"\t"` を text として届けるが、これらは `KeyInput` 側で処理済みで、
///   テキストとして挿入してはいけない。
/// - **Cmd (meta) 押下中は全部落とす。** `Cmd+C` / `Cmd+V` はショートカットで
///   あってテキスト入力ではないのに、macOS は `event.text` に文字を載せてくる。
///   ゲートしないと focus 中のフィールドやターミナルに文字が漏れる。
/// - **Ctrl は明示的にはゲートしない。** Ctrl 併用は制御文字として届くので、
///   上の制御文字フィルタで落ちる。
/// - **Alt は通す。** Option/Alt は実在のテキスト（`é`, `©`, …）を作るので、
///   ここで殺すとヨーロッパ言語のキーボードでまともに入力できなくなる。
///   Option-as-Meta が要るアプリは `KeyInput` 側の修飾子を見て判断する。
/// - **physical key が `Unidentified` のイベントは無視する。** IME 合成中の
///   synthetic event を弾くため（確定は `Ime::Commit` 経路が担当する）。
pub fn char_inputs(event: &KeyEvent, modifiers: Modifiers) -> Vec<char> {
    if event.state != ElementState::Pressed {
        return Vec::new();
    }
    if modifiers.meta {
        return Vec::new();
    }
    if matches!(event.physical_key, PhysicalKey::Unidentified(_)) {
        return Vec::new();
    }
    let Some(text) = event.text.as_ref() else {
        return Vec::new();
    };
    text.chars().filter(|ch| !ch.is_control()).collect()
}

/// winit の IME イベントを [`InputEvent`](sabitori_input::InputEvent) へ変換する。
///
/// **`Ime::Disabled` は空の `ImePreedit` にする。** 変換中に入力ソースが
/// 変わると（別アプリで英数に切り替えて戻ってきた等）、winit は最初の
/// `keyDown:` で `Ime::Disabled` を送るだけで、marked text の確定も取り消しも
/// 知らせない。ここで捨てるとアプリは「変換が終わった」ことを一度も受け取らず、
/// 自前で描いている preedit が浮いたまま残る (#124)。空の preedit は既存の
/// アプリがすでに「変換を消す」として扱っている形なので、アプリ側の変更は要らない。
pub fn input_from_ime(ime: &Ime) -> sabitori_input::InputEvent {
    use sabitori_input::InputEvent;
    match ime {
        Ime::Enabled => InputEvent::ImeEnabled,
        Ime::Preedit(text, cursor) => InputEvent::ImePreedit {
            text: text.clone(),
            cursor: *cursor,
        },
        Ime::Commit(text) => InputEvent::ImeCommit { text: text.clone() },
        Ime::Disabled => InputEvent::ImePreedit {
            text: String::new(),
            cursor: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 各 [`Key`] に対応する winit 側の入力を返す。
    ///
    /// **この `match` は網羅的なので、`Key` に variant を足すとコンパイルが落ちる。**
    /// それが狙い — enum に足して `key_from_winit` への配線を忘れる、という
    /// 今回の事故（`F1`〜`F12` / `PageUp` / `Insert` が 1 ランタイムにしか
    /// 配線されなかった）をビルド時に止める。
    fn winit_source_of(key: Key) -> Option<WinitKey> {
        let named = |n: NamedKey| Some(WinitKey::Named(n));
        let ch = |s: &str| Some(WinitKey::Character(s.into()));
        match key {
            Key::Backspace => named(NamedKey::Backspace),
            Key::Delete => named(NamedKey::Delete),
            Key::Left => named(NamedKey::ArrowLeft),
            Key::Right => named(NamedKey::ArrowRight),
            Key::Up => named(NamedKey::ArrowUp),
            Key::Down => named(NamedKey::ArrowDown),
            Key::Home => named(NamedKey::Home),
            Key::End => named(NamedKey::End),
            Key::Enter => named(NamedKey::Enter),
            Key::Tab => named(NamedKey::Tab),
            Key::Escape => named(NamedKey::Escape),
            Key::PageUp => named(NamedKey::PageUp),
            Key::PageDown => named(NamedKey::PageDown),
            Key::Insert => named(NamedKey::Insert),
            Key::F1 => named(NamedKey::F1),
            Key::F2 => named(NamedKey::F2),
            Key::F3 => named(NamedKey::F3),
            Key::F4 => named(NamedKey::F4),
            Key::F5 => named(NamedKey::F5),
            Key::F6 => named(NamedKey::F6),
            Key::F7 => named(NamedKey::F7),
            Key::F8 => named(NamedKey::F8),
            Key::F9 => named(NamedKey::F9),
            Key::F10 => named(NamedKey::F10),
            Key::F11 => named(NamedKey::F11),
            Key::F12 => named(NamedKey::F12),
            Key::Space => named(NamedKey::Space),
            Key::Shift => named(NamedKey::Shift),
            Key::A => ch("a"),
            Key::B => ch("b"),
            Key::C => ch("c"),
            Key::D => ch("d"),
            Key::E => ch("e"),
            Key::F => ch("f"),
            Key::G => ch("g"),
            Key::H => ch("h"),
            Key::I => ch("i"),
            Key::J => ch("j"),
            Key::K => ch("k"),
            Key::L => ch("l"),
            Key::M => ch("m"),
            Key::N => ch("n"),
            Key::O => ch("o"),
            Key::P => ch("p"),
            Key::Q => ch("q"),
            Key::R => ch("r"),
            Key::S => ch("s"),
            Key::T => ch("t"),
            Key::U => ch("u"),
            Key::V => ch("v"),
            Key::W => ch("w"),
            Key::X => ch("x"),
            Key::Y => ch("y"),
            Key::Z => ch("z"),
            Key::Digit0 => ch("0"),
            Key::Digit1 => ch("1"),
            Key::Digit2 => ch("2"),
            Key::Digit3 => ch("3"),
            Key::Digit4 => ch("4"),
            Key::Digit5 => ch("5"),
            Key::Digit6 => ch("6"),
            Key::Digit7 => ch("7"),
            Key::Digit8 => ch("8"),
            Key::Digit9 => ch("9"),
            Key::Period => ch("."),
            Key::Comma => ch(","),
            Key::Slash => ch("/"),
            Key::Minus => ch("-"),
            Key::Equal => ch("="),
            Key::BracketLeft => ch("["),
            Key::BracketRight => ch("]"),
            // 「対応する winit の入力が無い」ことを意味する受け皿。
            Key::Other => None,
        }
    }

    /// `Key` の全 variant が winit から到達できること。
    ///
    /// `Key::ALL` に足し忘れた場合はここでは検出できないが、その前に
    /// `winit_source_of` の網羅 match がコンパイルを止める。
    #[test]
    fn every_key_is_reachable_from_winit() {
        for &key in Key::ALL {
            let Some(source) = winit_source_of(key) else {
                continue;
            };
            assert_eq!(
                key_from_winit(&source),
                Some(key),
                "{key:?} が key_from_winit に配線されていない",
            );
        }
    }

    /// ⇧ で字が変わった記号も、⇧ 無しと同じ `Key` に落ちること。
    #[test]
    fn shifted_symbols_map_to_the_unshifted_key() {
        // ⇧⌘. は logical が `>` で来ることがある。⌘ のショートカットとして同じキー
        for (shifted, key) in [
            (">", Key::Period),
            ("{", Key::BracketLeft),
            ("}", Key::BracketRight),
            ("?", Key::Slash),
        ] {
            assert_eq!(key_from_winit(&WinitKey::Character(shifted.into())), Some(key));
        }
    }

    #[test]
    fn physical_digits_cover_shifted_and_foreign_layouts() {
        // ⇧1 は `!` (US) / JIS でも位置は同じ。文字で決まらない分は物理キーで当てる
        assert_eq!(key_from_winit(&WinitKey::Character("!".into())), Some(Key::Other));
        assert_eq!(physical_key(KeyCode::Digit1), Some(Key::Digit1));
        assert_eq!(physical_key(KeyCode::Period), Some(Key::Period));
        assert_eq!(physical_key(KeyCode::KeyA), None, "文字キーは文字で決める");
    }

    /// 大文字の文字キーも同じ `Key` に落ちること（Shift 併用時）。
    #[test]
    fn uppercase_characters_map_to_the_same_key() {
        assert_eq!(key_from_winit(&WinitKey::Character("A".into())), Some(Key::A));
        assert_eq!(key_from_winit(&WinitKey::Character("Z".into())), Some(Key::Z));
    }

    /// 未知の名前付きキーは `None`（＝イベントを出さない選択肢を呼び出し側に残す）、
    /// 未知の文字キーは `Some(Key::Other)`。
    #[test]
    fn unknown_keys_keep_the_named_character_distinction() {
        assert_eq!(key_from_winit(&WinitKey::Named(NamedKey::F13)), None);
        assert_eq!(
            key_from_winit(&WinitKey::Character("@".into())),
            Some(Key::Other),
        );
    }

    #[test]
    fn modifiers_round_trip() {
        let m = modifiers_from_winit(ModifiersState::SHIFT | ModifiersState::CONTROL);
        assert!(m.shift && m.ctrl);
        assert!(!m.alt && !m.meta);
    }

    /// 変換中に入力ソースが変わったときの `Ime::Disabled` を捨てず、
    /// 「変換が消えた」として届ける (#124)。
    #[test]
    fn ime_disabled_becomes_empty_preedit() {
        assert!(matches!(
            input_from_ime(&Ime::Disabled),
            sabitori_input::InputEvent::ImePreedit { text, cursor: None } if text.is_empty()
        ));
    }
}
