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
/// 変換が途切れる。ただし改行 (送った 1 行) があれば、変換中でも見せ直す。
/// 送った文が IME の手元に残っていると、それを確定し直して、もう一度打たれる
/// (外部キーボードで日本語を打つと、送った文が次の入力欄に残った)。外部
/// キーボードの打鍵には、GameTextInput が毎回「変換中」の印を付けるので、
/// 変換中を待つといつまでも見せ直せない。
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
    let sent = out.contains(&KbEvent::Enter);
    (out, sent || (!composing && new_body.len() > KEEP))
}

/// IME の文字列の読み手。
///
/// 見せ直し ([`ANCHOR`] だけに戻す) は Java の UI スレッドで**後から**行われ、
/// 頼んでもすぐには文字列が変わらない。届く前に読んだ文字列を「見せ直したあと」
/// と比べると、見せ直す前の文字列をまるごと新しい入力と読んでしまう (送った
/// 1 行がもう一度送られ、起動直後には消すキーが何十回も押された)。なので頼んだ
/// ときの文字列を覚えておき、そこから変わるまでは、まだ届いていないと読む。
#[derive(Debug)]
pub struct Reader {
    /// 前回読んだ文字列。
    last: String,
    /// 見せ直しを頼んで、まだ届いていない: 頼んだときの文字列。
    pending: Option<String>,
}

impl Reader {
    /// 見せ直しを頼んだところから読み始める。`now` はそのときの文字列。
    pub fn after_reset(now: &str) -> Self {
        Reader { last: now.to_string(), pending: Some(now.to_string()) }
    }

    /// 文字列が `now` (`composing`: 変換中) になっていた。打鍵に直したものと、
    /// 見せ直しを頼むべきかどうか。頼むなら、届くまで次は頼まない。
    pub fn read(&mut self, now: &str, composing: bool) -> (Vec<KbEvent>, bool) {
        if now == self.last {
            return (Vec::new(), false);
        }
        if let Some(stale) = &self.pending {
            // 届く前に IME が書き足しただけなら、頼んだときの文字列が頭に残っている。
            // 先頭の 1 文字だけ・それが消えた文字列 (空など) は、見せ直したあとの
            // 文字列にも頭に付くので当てない。
            let not_yet = stale != ANCHOR && stale.starts_with(ANCHOR) && now.starts_with(stale.as_str());
            if !not_yet {
                self.pending = None;
                self.last = ANCHOR.to_string();
            }
        }
        let (events, again) = delta(&self.last, now, composing);
        self.last = now.to_string();
        let ask = again && self.pending.is_none();
        if ask {
            self.pending = Some(now.to_string());
        }
        (events, ask)
    }
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

    use super::{ANCHOR, KbEvent, Reader};

    thread_local! {
        /// `None`: キーボードを出していない。
        static READER: RefCell<Option<Reader>> = const { RefCell::new(None) };
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
        READER.with(|reader| {
            let mut reader = reader.borrow_mut();
            if active == reader.is_some() {
                return;
            }
            if active {
                app.set_ime_editor_info(
                    InputType::TYPE_CLASS_TEXT | InputType::TYPE_TEXT_FLAG_MULTI_LINE,
                    TextInputAction::None,
                    ImeOptions::IME_FLAG_NO_FULLSCREEN,
                );
                reset(app);
                *reader = Some(Reader::after_reset(&app.text_input_state().text));
            } else {
                *reader = None;
            }
        });
    }

    /// 前回からの入力。
    pub fn drain(app: &AndroidApp) -> Vec<KbEvent> {
        READER.with(|reader| {
            let mut reader = reader.borrow_mut();
            let Some(reader) = reader.as_mut() else { return Vec::new() };
            let state = app.text_input_state();
            let (events, ask) = reader.read(&state.text, state.compose_region.is_some());
            if ask {
                reset(app);
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
        assert_eq!(delta(" ls", " ls\n", false), (vec![KbEvent::Enter], true));
        assert_eq!(delta(" ", " a\nb", false), (vec![text("a"), KbEvent::Enter, text("b")], true));
    }

    /// 送った 1 行は IME に見せたままにしない。変換中の印が付いていても。
    #[test]
    fn a_sent_line_is_taken_back_from_the_ime_even_mid_conversion() {
        assert_eq!(delta(" 送る", " 送る\n", true), (vec![KbEvent::Enter], true));
    }

    /// 送った 1 行は、見せ直しが IME に届くまで読み直しても、もう一度は送らない
    /// (実機で `echo Q7` が 2 回実行された)。
    #[test]
    fn a_sent_line_is_sent_once_while_the_reset_is_on_its_way() {
        let mut r = Reader::after_reset(" ");
        assert_eq!(r.read(" ", false), (vec![], false));
        assert_eq!(r.read(" ls", false), (vec![text("ls")], false));
        assert_eq!(r.read(" ls\n", false), (vec![KbEvent::Enter], true));
        // まだ届いていない。
        assert_eq!(r.read(" ls\n", false), (vec![], false));
        // 届いた。
        assert_eq!(r.read(" ", false), (vec![], false));
        assert_eq!(r.read(" a", false), (vec![text("a")], false));
    }

    /// 起動直後の空の文字列は、見せ直しが届くまで「消すキー」と読まない
    /// (実機で起動のたびに 31 回押されていた)。
    #[test]
    fn the_empty_text_before_the_first_reset_lands_is_not_a_backspace() {
        let mut r = Reader::after_reset("");
        assert_eq!(r.read("", false), (vec![], false));
        assert_eq!(r.read(" ", false), (vec![], false));
        assert_eq!(r.read(" a", false), (vec![text("a")], false));
    }

    /// 届いた直後に打たれていても、見せ直したあとの文字列として読む。
    #[test]
    fn typing_right_after_the_reset_lands_is_read_from_the_anchor() {
        let mut r = Reader::after_reset(" ");
        r.read(" ls\n", false);
        assert_eq!(r.read(" a", false), (vec![text("a")], false));
    }

    /// 届く前に IME が書き足したぶんは、その分だけ打つ。2 行目を送っても
    /// 見せ直しは重ねて頼まない (後から届いた 2 回目が、打ったばかりの字を消す)。
    #[test]
    fn what_the_ime_adds_before_the_reset_lands_is_typed_once() {
        let mut r = Reader::after_reset(" ");
        assert_eq!(r.read(" ls\n", false), (vec![text("ls"), KbEvent::Enter], true));
        assert_eq!(r.read(" ls\npwd\n", false), (vec![text("pwd"), KbEvent::Enter], false));
        assert_eq!(r.read(" ", false), (vec![], false));
        assert_eq!(r.read(" b", false), (vec![text("b")], false));
    }

    #[test]
    fn a_long_committed_text_is_shown_afresh_but_never_mid_conversion() {
        let long = format!(" {}", "x".repeat(KEEP + 1));
        assert!(delta(" ", &long, false).1);
        assert!(!delta(" ", &long, true).1);
    }
}
