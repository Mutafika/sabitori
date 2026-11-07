//! 確かめのダイアログ (OS 標準の窓)。
//!
//! 文言とボタンを渡すと出し、押されたボタンを後で
//! [`DeclarativeApp::on_dialog_answered`] に届ける。届け方は [`crate::files::pick`]
//! と同じで、`key` の札で「どの問いの答えか」を見分ける。
//!
//! ```ignore
//! sabitori::dialog::ask(
//!     "close",
//!     Ask::new("「見積.csv」への変更を保存しますか？")
//!         .detail("保存しないと、変更は失われます。")
//!         .button("保存")
//!         .button("保存しない")
//!         .cancel_button("キャンセル"),
//! );
//!
//! // DeclarativeApp
//! fn on_dialog_answered(&mut self, key: &str, answer: Answer) {
//!     if key == "close" && answer == Answer::Button(0) { /* 保存して閉じる */ }
//! }
//! ```
//!
//! 何を聞くか (保存するか・消してよいか) はアプリが決める。ここは窓を出して
//! 答えを返すだけ。
//!
//! | | |
//! |---|---|
//! | macOS | `NSAlert` |
//! | web / Windows / Linux | **未対応** ([`Answer::Unsupported`]) — アプリが自分の画面で聞く |
//!
//! [`DeclarativeApp::on_dialog_answered`]: crate::DeclarativeApp::on_dialog_answered

use std::cell::RefCell;

/// 何を聞くか。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    /// 問い (太字の 1 行目)。
    pub message: String,
    /// 補足 (空なら出さない)。
    pub detail: String,
    /// ボタンの文字。**1 つ目が既定** (Return で押される)。空なら「OK」1 つ。
    pub buttons: Vec<String>,
    /// Esc で押されるボタン。
    pub cancel: Option<usize>,
}

impl Ask {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), ..Self::default() }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// ボタンを足す (足した順に 0, 1, 2…)。
    pub fn button(mut self, label: impl Into<String>) -> Self {
        self.buttons.push(label.into());
        self
    }

    /// Esc でも押されるボタンを足す。
    pub fn cancel_button(mut self, label: impl Into<String>) -> Self {
        self.cancel = Some(self.buttons.len());
        self.buttons.push(label.into());
        self
    }
}

/// 答え。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// 押されたボタン ([`Ask::buttons`] の何番目か)。
    Button(usize),
    /// この環境には OS のダイアログが無い。アプリが自分の画面で聞く。
    Unsupported,
}

thread_local! {
    /// 届いた答え。ランタイムが毎フレーム汲んで `on_dialog_answered` を呼ぶ。
    static RESULTS: RefCell<Vec<(String, Answer)>> = const { RefCell::new(Vec::new()) };
}

fn deliver(key: String, answer: Answer) {
    RESULTS.with(|r| r.borrow_mut().push((key, answer)));
    #[cfg(target_arch = "wasm32")]
    crate::web_wake::wake();
}

/// ランタイムが毎フレーム汲む。
pub(crate) fn take_results() -> Vec<(String, Answer)> {
    RESULTS.with(|r| std::mem::take(&mut *r.borrow_mut()))
}

/// **テストから「このボタンが押された」を差し込む。** ダイアログは OS のものなので、
/// ヘッドレスのテストからは開けない。
pub fn test_deliver(key: impl Into<String>, answer: Answer) {
    deliver(key.into(), answer);
}

/// ダイアログを出す。答えは [`DeclarativeApp::on_dialog_answered`] に届く。
///
/// macOS ではその場で出して、閉じるまで待つ (`runModal`)。メインスレッド (クリック
/// 処理や `on_close_requested` の中) から呼ぶ。
///
/// [`DeclarativeApp::on_dialog_answered`]: crate::DeclarativeApp::on_dialog_answered
pub fn ask(key: impl Into<String>, ask: Ask) {
    let key = key.into();
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    {
        deliver(key, macos::ask(&ask));
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = &ask;
        deliver(key, Answer::Unsupported);
    }
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    {
        let _ = &ask;
        tracing::warn!("dialog::ask はこの環境では未対応");
        deliver(key, Answer::Unsupported);
    }
}

#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
mod macos {
    use super::{Answer, Ask};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn};
    use objc2_foundation::NSString;

    pub fn ask(ask: &Ask) -> Answer {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!("dialog::ask をメインスレッド以外から呼んでいる");
            return Answer::Unsupported;
        };
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(&ask.message));
        if !ask.detail.is_empty() {
            alert.setInformativeText(&NSString::from_str(&ask.detail));
        }
        for (i, label) in ask.buttons.iter().enumerate() {
            let button = alert.addButtonWithTitle(&NSString::from_str(label));
            // 1 つ目は Return のまま。それ以外で Esc に当てたいものに付ける。
            if ask.cancel == Some(i) && i != 0 {
                button.setKeyEquivalent(&NSString::from_str("\u{1b}"));
            }
        }
        let response = alert.runModal();
        let i = (response - NSAlertFirstButtonReturn).max(0) as usize;
        Answer::Button(i.min(ask.buttons.len().saturating_sub(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_marks_the_cancel_button() {
        let a = Ask::new("保存しますか？").button("保存").button("保存しない").cancel_button("キャンセル");
        assert_eq!(a.buttons, ["保存", "保存しない", "キャンセル"]);
        assert_eq!(a.cancel, Some(2));
    }

    #[test]
    fn test_deliver_reaches_take_results() {
        test_deliver("k", Answer::Button(1));
        assert_eq!(take_results(), [("k".to_string(), Answer::Button(1))]);
        assert!(take_results().is_empty());
    }
}
