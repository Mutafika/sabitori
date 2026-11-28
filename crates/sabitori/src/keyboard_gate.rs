//! 携帯のソフトキーボードを、いつ出していつしまうか ([`KeyboardGate`])。
//!
//! 判断だけを持ち、UIKit も winit も触らないので、どの OS のテストでも回る。
//! CI は iOS をビルドしないため、`ios_keyboard` の側はここの答えどおりに
//! `set_active` するだけにしてある。
//!
//! # 決まり
//!
//! 出すのは、利用者の入力に結びつくときだけ:
//! - 焦点を受ける所をタップした ([`KeyboardGate::on_tap`])。
//! - タップ・クリック・キーの直後 ([`AFTER_INPUT`] のうち) に、焦点が別の id へ
//!   移った。ボタンを押すと検索欄が開いて次の build で焦点が来る、メニューの
//!   「名前を変更」で名前の欄に焦点が来る、など。
//!
//! 出さないのは:
//! - 入力と結びつかない焦点の移動。起動時から決まっている焦点 (端末)、
//!   よそ (別の機械・別スレッド) の出来事でペインが閉じて焦点が隣へ移った、など。
//! - 焦点が同じ id のまま。利用者がキーボードの「閉じる」でしまった後に、
//!   勝手にせり上げ直さない。
//!
//! しまうのは、焦点が無いか、アプリが文字を受けない (`ime_allowed` が偽) とき。
//! 焦点を受けるだけの物 (矢印キーで動く一覧の地など) をタップしたときは、
//! アプリが `ime_allowed` を偽にして出さない。

use std::time::Duration;
use web_time::Instant;

/// 入力の後、焦点の移動をその入力の結果と見なす長さ。焦点は押した次の
/// build で移る (数フレーム)。寝ている携帯は描かない間フレームが進まないので、
/// フレームではなく時間で数える — フレームで数えると、タップの後に描かないまま
/// 何秒もたってからの、入力と関係ない移動まで拾う。
pub(crate) const AFTER_INPUT: Duration = Duration::from_millis(500);

/// [`KeyboardGate`] の答え。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyboardAction {
    /// 出す。
    Show,
    /// しまう。
    Hide,
    /// 今のまま (出ていれば出たまま、しまってあればしまったまま)。
    Keep,
}

/// ソフトキーボードを出す・しまうの判断。
#[derive(Debug, Default)]
pub(crate) struct KeyboardGate {
    /// 前のフレームで見た焦点。
    focus: Option<String>,
    /// 利用者の最後の入力 (指を離した・クリック・キー)。
    last_input: Option<Instant>,
    /// 入力の直後に焦点が別の id へ移り、まだ出していない。アプリが文字を
    /// 受けると言うのが焦点より 1 フレーム遅れても出せるよう、移った時に
    /// 覚えておく。
    armed: bool,
}

impl KeyboardGate {
    /// 利用者の入力があった (指を離した・クリックを離した・キーを押した)。
    pub(crate) fn note_input(&mut self, now: Instant) {
        self.last_input = Some(now);
    }

    /// 指を離してタップになった。押した所が焦点を受ける所
    /// (`tap_wants_keyboard`) で、アプリが文字を受けるなら出す。
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub(crate) fn on_tap(&self, tap_wants_keyboard: bool, ime_allowed: bool) -> KeyboardAction {
        if tap_wants_keyboard && ime_allowed {
            KeyboardAction::Show
        } else {
            KeyboardAction::Keep
        }
    }

    /// 毎フレーム、今の焦点とアプリの `ime_allowed` から決める。
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    pub(crate) fn on_frame(
        &mut self,
        focused: Option<&str>,
        ime_allowed: bool,
        now: Instant,
    ) -> KeyboardAction {
        let after_input = self
            .last_input
            .is_some_and(|t| now.saturating_duration_since(t) <= AFTER_INPUT);
        if focused != self.focus.as_deref() {
            self.focus = focused.map(str::to_owned);
            self.armed = focused.is_some() && after_input;
        }
        if !after_input {
            self.armed = false;
        }
        if !(ime_allowed && focused.is_some()) {
            return KeyboardAction::Hide;
        }
        if std::mem::take(&mut self.armed) {
            KeyboardAction::Show
        } else {
            KeyboardAction::Keep
        }
    }
}

#[cfg(test)]
mod tests {
    use super::KeyboardAction::{Hide, Keep, Show};
    use super::*;

    fn ms(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// 焦点を受ける所をタップしたら出す。受けない所や、アプリが文字を受けない
    /// ときは出さない。
    #[test]
    fn a_tap_on_a_field_shows_it() {
        let gate = KeyboardGate::default();
        assert_eq!(gate.on_tap(true, true), Show);
        assert_eq!(gate.on_tap(false, true), Keep, "ボタン");
        assert_eq!(gate.on_tap(true, false), Keep, "焦点を受けるだけの一覧の地");
    }

    /// ボタンを押した次の build で焦点が欄へ移った (検索・名前の変更): 出す。
    /// アプリが文字を受けると言うのが 1 フレーム遅れても出す。
    #[test]
    fn focus_moved_by_a_tap_shows_it() {
        let t0 = Instant::now();
        let mut gate = KeyboardGate::default();
        assert_eq!(gate.on_frame(Some("list"), false, t0), Hide);
        gate.note_input(ms(t0, 1000));
        assert_eq!(
            gate.on_frame(Some("list"), false, ms(t0, 1008)),
            Hide,
            "押した直後はまだ移っていない"
        );
        assert_eq!(gate.on_frame(Some("search"), true, ms(t0, 1016)), Show);
        assert_eq!(
            gate.on_frame(Some("search"), true, ms(t0, 1024)),
            Keep,
            "1 度だけ"
        );

        // 欄に焦点が来たフレームでは、まだアプリが文字を受けると言っていない
        gate.note_input(ms(t0, 2000));
        assert_eq!(gate.on_frame(Some("rename"), false, ms(t0, 2008)), Hide);
        assert_eq!(gate.on_frame(Some("rename"), true, ms(t0, 2016)), Show);
    }

    /// 入力と結びつかない焦点の移動では出さない: 起動時の焦点、よその出来事で
    /// ペインが閉じて隣へ移った、入力からしばらくたってからの移動。
    #[test]
    fn focus_moved_without_input_does_not_show_it() {
        let t0 = Instant::now();
        let mut gate = KeyboardGate::default();
        assert_eq!(
            gate.on_frame(Some("term-1"), true, t0),
            Keep,
            "起動時から決まっている焦点"
        );
        assert_eq!(
            gate.on_frame(Some("term-2"), true, ms(t0, 3000)),
            Keep,
            "ペインが閉じて隣へ"
        );
        gate.note_input(ms(t0, 4000));
        assert_eq!(gate.on_frame(Some("term-2"), true, ms(t0, 4008)), Keep);
        assert_eq!(
            gate.on_frame(Some("term-3"), true, ms(t0, 9000)),
            Keep,
            "入力から 5 秒後の移動"
        );
    }

    /// 利用者がキーボードの「閉じる」でしまった後は、焦点が同じ id のままなら
    /// 出し直さない。もう 1 度タップすれば出る。
    #[test]
    fn the_same_focus_after_a_dismiss_does_not_show_it_again() {
        let t0 = Instant::now();
        let mut gate = KeyboardGate::default();
        gate.note_input(t0);
        assert_eq!(gate.on_frame(Some("search"), true, ms(t0, 8)), Show);
        // ここで利用者が「閉じる」を押す (UIKit の側で欄が first responder を降りる)
        gate.note_input(ms(t0, 100));
        for f in 1..10 {
            assert_eq!(
                gate.on_frame(Some("search"), true, ms(t0, 100 + f * 8)),
                Keep
            );
        }
        assert_eq!(gate.on_tap(true, true), Show, "欄をタップし直したら出る");
    }

    /// 焦点が無くなるか、アプリが文字を受けなくなったらしまう。
    #[test]
    fn no_focus_or_no_text_hides_it() {
        let t0 = Instant::now();
        let mut gate = KeyboardGate::default();
        gate.note_input(t0);
        assert_eq!(gate.on_frame(Some("search"), true, ms(t0, 8)), Show);
        assert_eq!(
            gate.on_frame(None, true, ms(t0, 16)),
            Hide,
            "Esc で欄を閉じた"
        );
        assert_eq!(
            gate.on_frame(Some("list"), false, ms(t0, 24)),
            Hide,
            "一覧の地"
        );
    }
}
