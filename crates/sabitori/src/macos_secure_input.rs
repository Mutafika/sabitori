//! 伏字の欄に焦点があるあいだ、macOS の「セキュアな入力」を立てる。
//!
//! 立っているあいだ、打鍵は前にいるこのアプリにしか流れない — 入力監視の権限を
//! 持つ他のアプリ (`CGEventTap` で全体の打鍵を見るもの) にも見えなくなる。
//! ブラウザも `<input type="password">` に焦点があるあいだ同じことをしている。
//! sabitori の伏字欄は OS から見ればふつうの view なので、ここで立てないと
//! 誰も立てない。
//!
//! `EnableSecureEventInput` は**回数を数える** (立てた回数だけ下ろさないと
//! 下りない)。なので今の状態をここで 1 つだけ持ち、変わったときにだけ呼ぶ。
//! 窓が複数あってもプロセスで 1 つなので、決めるのは窓を束ねる `Host`。
//!
//! 下ろし忘れると、窓を離れたあとも他のアプリの打鍵の監視が止まったままになる
//! (ショートカット系のアプリが「〜がセキュリティ保護された入力を使用中」と出す)。
//! なので窓が後ろに回ったら下ろす。

#![cfg(target_os = "macos")]

use std::cell::Cell;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn EnableSecureEventInput() -> i32;
    fn DisableSecureEventInput() -> i32;
    #[cfg(test)]
    fn IsSecureEventInputEnabled() -> u8;
}

thread_local! {
    /// このプロセスが立てているか。
    static ON: Cell<bool> = const { Cell::new(false) };
}

/// 立てる / 下ろす。今と同じなら何もしない (回数を釣り合わせるため)。
pub(crate) fn set(on: bool) {
    ON.with(|cur| {
        if cur.get() == on {
            return;
        }
        cur.set(on);
        // SAFETY: 引数も戻り値の所有権も無い Carbon の関数。
        let status = unsafe { if on { EnableSecureEventInput() } else { DisableSecureEventInput() } };
        if status != 0 {
            log::warn!("macos_secure_input: {} が {status} を返した", if on { "Enable" } else { "Disable" });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> bool {
        // SAFETY: 引数の無い問い合わせ。
        unsafe { IsSecureEventInputEnabled() != 0 }
    }

    /// 何度立てても、1 回下ろせば下りる。回数が釣り合わないと、窓を離れても
    /// 他のアプリの打鍵の監視が止まったままになる。
    #[test]
    fn repeated_requests_stay_balanced() {
        // 問い合わせはシステム全体の状態を返す。他のアプリ (パスワード欄に焦点が
        // ある別の窓など) が立てていれば、こちらが下ろしても立ったまま。
        let before = enabled();
        set(true);
        set(true);
        assert!(enabled(), "立てたら立つ");
        set(false);
        assert_eq!(enabled(), before, "1 回下ろせば元に戻る");
        set(false);
        assert_eq!(enabled(), before, "余分に下ろしても何も起きない");
    }
}
