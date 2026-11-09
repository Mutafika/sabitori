//! 確かめのダイアログ。
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
//! # どこに出るか
//!
//! | | |
//! |---|---|
//! | macOS | `NSAlert` (OS の確かめの窓)。[`DeclarativeApp::native_dialogs`] が `false` なら窓の中に描く |
//! | web / Windows / Linux | 窓の中に描く |
//!
//! 窓の中に描くときは、ランタイムが [`ask_dialog`](sabitori_widgets::ask_dialog) を
//! 窓の上に重ねる。見た目は [`DeclarativeApp::ask_dialog_style`] (既定は `ctx.theme`
//! の色)。1 つ目のボタンが既定で右端・Return で押され、[`Ask::cancel_button`] は
//! Esc で押される。出ている間、キーと下の画面のクリックはダイアログが吸う。
//! 出している間に聞いた問いは、答えた後に順に出る。
//!
//! [`DeclarativeApp::on_dialog_answered`]: crate::DeclarativeApp::on_dialog_answered
//! [`DeclarativeApp::native_dialogs`]: crate::DeclarativeApp::native_dialogs
//! [`DeclarativeApp::ask_dialog_style`]: crate::DeclarativeApp::ask_dialog_style

use std::cell::{Cell, RefCell};

/// 何を聞くか。窓の中に描く部品 ([`sabitori_widgets::ask_dialog`]) と同じ型。
pub use sabitori_widgets::Ask;

/// 答え。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// 押されたボタン ([`Ask::buttons`] の何番目か)。
    Button(usize),
    /// 出せなかった (メインスレッド以外から聞いた)。
    Unsupported,
}

thread_local! {
    /// 届いた答え。ランタイムが毎フレーム汲んで `on_dialog_answered` を呼ぶ。
    static RESULTS: RefCell<Vec<(String, Answer)>> = const { RefCell::new(Vec::new()) };
    /// 窓の中に描く問い。ランタイムが毎フレーム汲んで重ねる。
    static DRAWN: RefCell<Vec<(String, Ask)>> = const { RefCell::new(Vec::new()) };
    /// OS の窓を使ってよいか (`DeclarativeApp::native_dialogs`)。ランタイムが
    /// 毎フレーム合わせる。
    static NATIVE: Cell<bool> = const { Cell::new(true) };
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

/// 窓の中に描く問いを、ランタイムが毎フレーム汲む。
pub(crate) fn take_drawn() -> Vec<(String, Ask)> {
    DRAWN.with(|d| std::mem::take(&mut *d.borrow_mut()))
}

/// OS の窓を使ってよいかを、ランタイムがアプリの設定に合わせる。
pub(crate) fn set_native(yes: bool) {
    NATIVE.with(|n| n.set(yes));
}

/// **テストから「このボタンが押された」を差し込む。** OS の窓はヘッドレスの
/// テストからは開けない。窓の中に描く方は `Harness` でボタンを押せばよい
/// (`"sabitori::ask::button::{n}"`)。
pub fn test_deliver(key: impl Into<String>, answer: Answer) {
    deliver(key.into(), answer);
}

/// ダイアログを出す。答えは [`DeclarativeApp::on_dialog_answered`] に届く。
///
/// macOS の OS の窓はその場で出して、閉じるまで待つ (`runModal`)。窓の中に描く
/// ときは次のフレームで出る。どちらもメインスレッド (クリック処理や
/// `on_close_requested` の中) から呼ぶ。
///
/// [`DeclarativeApp::on_dialog_answered`]: crate::DeclarativeApp::on_dialog_answered
pub fn ask(key: impl Into<String>, ask: Ask) {
    let key = key.into();
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    if NATIVE.with(Cell::get) {
        deliver(key, macos::ask(&ask));
        return;
    }
    DRAWN.with(|d| d.borrow_mut().push((key, ask)));
    #[cfg(target_arch = "wasm32")]
    crate::web_wake::wake();
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
    fn drawn_asks_wait_for_the_runtime() {
        set_native(false);
        ask("k", Ask::new("消しますか？").button("消す"));
        set_native(true);
        let drawn = take_drawn();
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0, "k");
        assert!(take_drawn().is_empty());
        assert!(take_results().is_empty(), "答えはまだ無い");
    }

    #[test]
    fn test_deliver_reaches_take_results() {
        test_deliver("k", Answer::Button(1));
        assert_eq!(take_results(), [("k".to_string(), Answer::Button(1))]);
        assert!(take_results().is_empty());
    }
}
