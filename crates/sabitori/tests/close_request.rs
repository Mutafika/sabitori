//! 閉じる前に聞く (`on_close_requested`) と、確かめのダイアログ (`dialog`) の答えの届き方。

use sabitori::dialog::{self, Answer, Ask};
use sabitori::testing::Harness;
use sabitori::*;

/// 「保存していない変更」があるあいだは閉じるのを断り、ダイアログの答えで閉じる、
/// という利用側の典型の形。
#[derive(Default)]
struct Editor {
    unsaved: bool,
    asked: Vec<CloseReason>,
    saved: bool,
    close: bool,
    pending: Option<Ask>,
}

impl DeclarativeApp for Editor {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div()
            .w_full()
            .h_full()
            .child(div().id("close-btn").w(Px(20.0)).h(Px(20.0)).window_action(WindowAction::Close))
    }

    fn on_close_requested(&mut self, reason: CloseReason) -> bool {
        self.asked.push(reason);
        if !self.unsaved {
            return true;
        }
        // 本物のアプリはここで `dialog::ask(..)` を出す。macOS ではテストの中でも OS の窓が
        // 開いて止まるので、テストでは聞いたことだけ覚え、答えは `test_deliver` で差し込む。
        self.pending = Some(Ask::new("保存しますか？").button("保存").button("保存しない").cancel_button("キャンセル"));
        false
    }

    fn on_dialog_answered(&mut self, key: &str, answer: Answer) {
        if key != "close" {
            return;
        }
        match answer {
            Answer::Button(0) => {
                self.saved = true;
                self.close = true;
            }
            Answer::Button(1) => self.close = true,
            _ => {}
        }
    }

    fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.close)
    }
}

#[test]
fn closes_when_the_app_does_not_object() {
    let mut h = Harness::new(Editor::default(), 200.0, 100.0);
    h.frame();
    assert!(h.request_close(CloseReason::Window));
    assert_eq!(h.app().asked, [CloseReason::Window]);
}

#[test]
fn an_app_can_refuse_and_close_after_the_answer() {
    let mut h = Harness::new(Editor { unsaved: true, ..Default::default() }, 200.0, 100.0);
    h.frame();
    assert!(!h.request_close(CloseReason::Quit));
    assert_eq!(h.app().asked, [CloseReason::Quit]);

    assert_eq!(h.app().pending.as_ref().map(|a| a.cancel), Some(Some(2)));
    // 「保存」が押されたことにする。
    dialog::test_deliver("close", Answer::Button(0));
    h.frame();
    assert!(h.app().saved);
    assert!(h.app().close, "答えを受けたアプリが自分で閉じる");
}

#[test]
fn the_titlebar_close_button_asks_too() {
    let mut h = Harness::new(Editor { unsaved: true, ..Default::default() }, 200.0, 100.0);
    h.frame();
    h.click("close-btn");
    assert_eq!(h.app().asked, [CloseReason::Window]);
    assert_eq!(h.take_window_action(), None, "断ったら窓の操作は掛けない");
}
