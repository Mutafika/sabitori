//! Android のソフトキーボードから文字を受け取る。
//!
//! winit は Android で IME のイベントを出さない。キーボードは
//! `Window::set_ime_allowed` で出し入れでき、英数は普通の打鍵として届くが、
//! **かな漢字変換の結果は打鍵にならない**。GameActivity (`android-game-activity`)
//! は IME が編集している文字列をまるごと持っているので、それを毎周回のぞいて、
//! 前回との差を「消す・足す」に直して流す。iOS (`ios_keyboard`) と同じやり方で、
//! 変換中の文字もそのまま打たれ、確定で打ち直される。
//!
//! NativeActivity にはその文字列が無い (いつも空) ので、何もしない。
//!
//! 文字列の先頭にはいつも [`ANCHOR`] を 1 文字置く。空の欄で消すキーを押しても
//! IME は「消すものが無い」と何も言ってこないので、消せる 1 文字を見せておき、
//! それが消えたら「消すキーが押された」と読む。

/// IME の編集文字列から読み取った入力。
#[derive(Debug, PartialEq)]
pub enum KbEvent {
    /// 足された文字。
    Text(String),
    /// 1 文字消された。
    Backspace,
    /// 改行キー。
    Enter,
}

/// 文字列の先頭に置く 1 文字 (モジュールの説明を参照)。
pub const ANCHOR: &str = " ";

/// 変換中でないとき、確定済みの文字がこれより長くなったら [`ANCHOR`] だけに戻す。
const KEEP: usize = 32;

/// IME の文字列が `old` から `new` になった。打鍵に直したものと、次に IME へ
/// 見せ直すべきかどうか (見せ直すなら [`ANCHOR`] だけにする)。
///
/// `composing` は変換中 (未確定の文字がある)。そのあいだは見せ直さない ──
/// 変換が途切れる。
pub fn delta(old: &str, new: &str, composing: bool) -> (Vec<KbEvent>, bool) {
    let mut out = Vec::new();
    let old_body: Vec<char> = old.strip_prefix(ANCHOR).unwrap_or(old).chars().collect();
    let Some(body) = new.strip_prefix(ANCHOR) else {
        // 置いておいた 1 文字ごと消された: 見えていたぶん全部と、もう 1 回。
        out.extend(old_body.iter().map(|_| KbEvent::Backspace));
        out.push(KbEvent::Backspace);
        push_text(&mut out, new);
        return (out, true);
    };
    let new_body: Vec<char> = body.chars().collect();
    let common = old_body.iter().zip(&new_body).take_while(|(a, b)| a == b).count();
    out.extend((common..old_body.len()).map(|_| KbEvent::Backspace));
    push_text(&mut out, &new_body[common..].iter().collect::<String>());
    (out, !composing && new_body.len() > KEEP)
}

/// `text` を、改行のところで切って足す。
fn push_text(out: &mut Vec<KbEvent>, text: &str) {
    let mut run = String::new();
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            if !run.is_empty() {
                out.push(KbEvent::Text(std::mem::take(&mut run)));
            }
            out.push(KbEvent::Enter);
        } else {
            run.push(ch);
        }
    }
    if !run.is_empty() {
        out.push(KbEvent::Text(run));
    }
}

#[cfg(all(target_os = "android", feature = "android-game-activity"))]
mod live {
    use std::cell::RefCell;

    use sabitori_window::android::AndroidApp;
    use winit::platform::android::activity::input::{ImeOptions, InputType, TextInputAction, TextInputState, TextSpan};

    use super::{ANCHOR, KbEvent, delta};

    thread_local! {
        /// 前回見た文字列。`None`: キーボードを出していない。
        static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    fn reset(app: &AndroidApp) {
        let at = ANCHOR.len();
        app.set_text_input_state(TextInputState {
            text: ANCHOR.to_string(),
            selection: TextSpan { start: at, end: at },
            compose_region: None,
        });
    }

    /// キーボードを出す / しまうのに合わせて呼ぶ。出すときに、改行キーが改行を
    /// 入れる・全画面の入力欄を出さない、と IME に伝える。
    pub fn set_active(app: &AndroidApp, active: bool) {
        LAST.with(|last| {
            let mut last = last.borrow_mut();
            if active == last.is_some() {
                return;
            }
            if active {
                app.set_ime_editor_info(
                    InputType::TYPE_CLASS_TEXT | InputType::TYPE_TEXT_FLAG_MULTI_LINE,
                    TextInputAction::None,
                    ImeOptions::IME_FLAG_NO_FULLSCREEN,
                );
                reset(app);
                *last = Some(ANCHOR.to_string());
            } else {
                *last = None;
            }
        });
    }

    /// 前回からの入力。
    pub fn drain(app: &AndroidApp) -> Vec<KbEvent> {
        LAST.with(|last| {
            let mut last = last.borrow_mut();
            let Some(old) = last.as_mut() else { return Vec::new() };
            let state = app.text_input_state();
            if state.text == *old {
                return Vec::new();
            }
            let (events, again) = delta(old, &state.text, state.compose_region.is_some());
            if again {
                reset(app);
                *old = ANCHOR.to_string();
            } else {
                *old = state.text;
            }
            events
        })
    }
}

#[cfg(all(target_os = "android", feature = "android-game-activity"))]
pub use live::{drain, set_active};

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> KbEvent {
        KbEvent::Text(s.to_string())
    }

    #[test]
    fn typed_letters_are_added() {
        assert_eq!(delta(" ", " a", false), (vec![text("a")], false));
        assert_eq!(delta(" a", " ab", false), (vec![text("b")], false));
    }

    #[test]
    fn a_conversion_retypes_what_it_replaces() {
        // 「にほん」と打って「日本」に確定する。
        assert_eq!(delta(" ", " に", true), (vec![text("に")], false));
        assert_eq!(delta(" にほん", " 日本", false), (
            vec![KbEvent::Backspace, KbEvent::Backspace, KbEvent::Backspace, text("日本")],
            false
        ));
    }

    #[test]
    fn deleting_past_the_start_is_a_backspace() {
        // 何も打っていない欄で消すキー: 置いておいた 1 文字が消える。
        assert_eq!(delta(" ", "", false), (vec![KbEvent::Backspace], true));
        // 打ったものがあれば、まずそれが消える。
        assert_eq!(delta(" ab", " a", false), (vec![KbEvent::Backspace], false));
        // IME がまるごと置き換えた。
        assert_eq!(delta(" ab", "x", false), (vec![KbEvent::Backspace, KbEvent::Backspace, KbEvent::Backspace, text("x")], true));
    }

    #[test]
    fn a_newline_is_the_enter_key() {
        assert_eq!(delta(" ls", " ls\n", false), (vec![KbEvent::Enter], false));
        assert_eq!(delta(" ", " a\nb", false), (vec![text("a"), KbEvent::Enter, text("b")], false));
    }

    #[test]
    fn a_long_committed_text_is_shown_afresh_but_never_mid_conversion() {
        let long = format!(" {}", "x".repeat(KEEP + 1));
        assert!(delta(" ", &long, false).1);
        assert!(!delta(" ", &long, true).1);
    }
}
