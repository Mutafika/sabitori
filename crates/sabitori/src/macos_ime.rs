//! macOS の入力モード切替を IME に確実に届ける。
//!
//! winit の view では、Ctrl+Space 等で入力ソース (例: 日本語 ⇄ 英字) を切り替えた直後の
//! 1 打が、15〜25% の確率で **1 つ前のモードのまま** IME に処理される (表示は切り替わって
//! いるのに「あ」のはずが `a`、`a` のはずが「あ」)。入力ソースの値自体は正しく変わっていて、
//! IMK セッションへのモード反映だけが取りこぼされる。
//!
//! 対策: keyDown が view に届く直前 (ローカルイベントモニタ) に入力ソースの変化を検知したら、
//! 現在の `NSTextInputContext` を deactivate → activate して IMK セッションを張り直す。
//! 切替の瞬間ではなく次の打鍵の直前に行うのは、切替時に張り直すと macOS の
//! 入力モード表示 (A/あ) が出なくなることがあるため。

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::Once;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSEventMask, NSTextInputContext};

thread_local! {
    /// 最後に keyDown を見た時点の入力ソース ID。
    static LAST_SOURCE: RefCell<Option<String>> = const { RefCell::new(None) };
}

static INSTALL: Once = Once::new();

/// プロセスに 1 度だけモニタを仕込む。IME を有効にするウィンドウ生成時に呼ぶ。
pub(crate) fn install_mode_resync() {
    INSTALL.call_once(|| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        LAST_SOURCE.with(|l| *l.borrow_mut() = current_source(mtm));
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            resync_if_source_changed(mtm);
            event.as_ptr()
        });
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
        // アプリの寿命中ずっと有効にしておく (removeMonitor しない)。
        std::mem::forget(monitor);
    });
}

fn resync_if_source_changed(mtm: MainThreadMarker) {
    let current = current_source(mtm);
    if !LAST_SOURCE.with(|l| source_changed(&mut l.borrow_mut(), current)) {
        return;
    }
    if let Some(ctx) = unsafe { NSTextInputContext::currentInputContext(mtm) } {
        unsafe {
            ctx.deactivate();
            ctx.activate();
        }
    }
}

fn current_source(mtm: MainThreadMarker) -> Option<String> {
    let ctx = unsafe { NSTextInputContext::currentInputContext(mtm) }?;
    unsafe { ctx.selectedKeyboardInputSource() }.map(|s| s.to_string())
}

/// 入力ソースが前回から変わったか。変わっていれば `last` を更新する。
/// 取得できなかった (`None`) 場合は変化とみなさない。
fn source_changed(last: &mut Option<String>, current: Option<String>) -> bool {
    match current {
        Some(cur) if last.as_deref() != Some(cur.as_str()) => {
            let was_known = last.is_some();
            *last = Some(cur);
            was_known
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::source_changed;

    const JA: &str = "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese";
    const EN: &str = "com.apple.inputmethod.Kotoeri.RomajiTyping.Roman";

    #[test]
    fn detects_switch_and_updates_last() {
        let mut last = Some(EN.to_string());
        assert!(source_changed(&mut last, Some(JA.to_string())));
        assert_eq!(last.as_deref(), Some(JA));
        assert!(source_changed(&mut last, Some(EN.to_string())));
    }

    #[test]
    fn same_source_is_not_a_change() {
        let mut last = Some(JA.to_string());
        assert!(!source_changed(&mut last, Some(JA.to_string())));
    }

    #[test]
    fn first_observation_only_records() {
        let mut last = None;
        assert!(!source_changed(&mut last, Some(JA.to_string())));
        assert_eq!(last.as_deref(), Some(JA));
    }

    #[test]
    fn unknown_current_is_ignored() {
        let mut last = Some(JA.to_string());
        assert!(!source_changed(&mut last, None));
        assert_eq!(last.as_deref(), Some(JA));
    }
}
