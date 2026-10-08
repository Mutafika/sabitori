//! 変換の途中で IME が切れたことが、 preedit を自前で描くアプリにも届くこと (#124)。
//!
//! ターミナルのように 「フォーカス要素は無いが IME 入力は受ける」 アプリは、
//! 変換中の文字を自分で描く。 `Ime::Disabled` を捨てていたころは、 変換が
//! 終わったという知らせ (空の preedit や確定) が一度も来ず、 消す機会が無かった。
//!
//! ランタイムは `Ime::Disabled` を空の preedit にして届ける
//! (`sabitori_window::keymap::input_from_ime`、 変換自体はそちらのテストが見る)。

use sabitori::testing::Harness;
use sabitori::*;

/// 受け取った preedit をそのまま覚えるだけのアプリ。
#[derive(Default)]
struct Terminal {
    preedit: String,
}

impl DeclarativeApp for Terminal {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div()
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::ImePreedit { text, .. } => {
                self.preedit = text.clone();
                true
            }
            _ => false,
        }
    }
}

#[test]
fn app_hears_that_composition_ended() {
    let mut h = Harness::new(Terminal::default(), 400.0, 200.0);
    h.frame();

    h.ime_preedit("にほん", None);
    assert_eq!(h.app().preedit, "にほん");

    h.ime_preedit("", None); // = Ime::Disabled
    assert_eq!(h.app().preedit, "", "変換が終わったことがアプリに届いていない");
}
