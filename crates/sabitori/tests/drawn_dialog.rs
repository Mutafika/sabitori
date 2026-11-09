//! 窓の中に描く確かめのダイアログ (`dialog::ask` + `native_dialogs() == false`)。
//! アプリは `dialog::ask` を呼んで `on_dialog_answered` で受けるだけで、重ねる・
//! キーを回す・答えを届けるのはランタイム。

use sabitori::dialog::{self, Answer, Ask};
use sabitori::testing::Harness;
use sabitori::*;

const BUTTON: &str = "sabitori::ask::button";

fn button(n: usize) -> String {
    format!("{BUTTON}::{n}")
}

struct App {
    name: TextInputState,
    answers: Vec<(String, Answer)>,
    behind_clicks: usize,
}

impl App {
    fn new() -> Self {
        Self { name: TextInputState::new("名前"), answers: Vec::new(), behind_clicks: 0 }
    }
}

impl DeclarativeApp for App {
    // macOS でも OS の窓 (テストの中で開いて止まる) を使わず、窓の中に描く。
    fn native_dialogs(&self) -> bool { false }

    fn view(&self, ctx: &ViewContext) -> Element {
        div().w_full().h_full().flex_col().children([
            text_input(ctx, "name", &self.name, &TextInputStyle::default_dark()),
            div().w(Px(200.0)).h(Px(200.0)).click(ctx, "behind", |app: &mut App| app.behind_clicks += 1),
            div().w(Px(60.0)).h(Px(30.0)).click(ctx, "delete", |_app: &mut App| ask_delete()),
        ])
    }

    fn on_dialog_answered(&mut self, key: &str, answer: Answer) {
        self.answers.push((key.to_string(), answer));
    }
}

fn ask_delete() {
    dialog::ask("delete", Ask::new("3 個の項目をゴミ箱に入れますか？")
        .detail("あとから戻せます。")
        .button("ゴミ箱に入れる")
        .cancel_button("キャンセル"));
}

fn open() -> Harness<App> {
    let mut h = Harness::new(App::new(), 600.0, 500.0);
    h.frame();
    h
}

#[test]
fn ask_is_drawn_in_the_window_and_the_pressed_button_comes_back() {
    let mut h = open();
    h.click("delete");
    h.settle();
    assert!(h.rect_of("sabitori::ask::dialog").is_some(), "窓の中に出る");
    let default = h.rect_of(&button(0)).expect("既定のボタン");
    let cancel = h.rect_of(&button(1)).expect("キャンセル");
    assert!(default.origin.x > cancel.origin.x, "既定が右端: {default:?} / {cancel:?}");

    h.click(&button(0));
    h.settle();
    assert_eq!(h.app().answers, [("delete".to_string(), Answer::Button(0))]);
    assert!(h.rect_of("sabitori::ask::dialog").is_none(), "閉じきったら描かない");
}

#[test]
fn the_dialog_takes_the_keys_and_the_clicks_while_it_is_up() {
    let mut h = open();
    // 欄に打っている途中で聞かれた (⌘⌫ や「閉じる前に聞く」と同じ形)。
    h.click("name");
    h.frame();
    ask_delete();
    h.settle();

    // 下の欄に文字が入らず、下の画面も押せない。背景を押しても閉じない。
    h.text("x");
    h.click_at(100.0, 150.0);
    h.settle();
    assert_eq!(h.app().name.text(), "");
    assert_eq!(h.app().behind_clicks, 0);
    assert!(h.rect_of("sabitori::ask::dialog").is_some());
    assert!(h.app().answers.is_empty());

    // Esc はキャンセルのボタン。
    h.key(Key::Escape, Modifiers::default());
    h.settle();
    assert_eq!(h.app().answers, [("delete".to_string(), Answer::Button(1))]);

    // 閉じたら、焦点はそのままで欄に戻る (ダイアログの上の押下では焦点を外さない)。
    assert_eq!(h.focused_id(), Some("name"));
    h.text("y");
    h.frame();
    assert_eq!(h.app().name.text(), "y");
}

#[test]
fn return_presses_the_default_and_questions_wait_their_turn() {
    let mut h = open();
    h.click("delete");
    h.frame();
    // 出している間にもう 1 つ聞く。
    dialog::ask("again", Ask::new("本当に？").button("はい").cancel_button("いいえ"));
    h.settle();
    h.key(Key::Enter, Modifiers::default());
    h.settle();
    assert_eq!(h.app().answers, [("delete".to_string(), Answer::Button(0))]);
    assert!(h.rect_of("sabitori::ask::dialog").is_some(), "次の問いが出ている");
    h.click(&button(1));
    h.settle();
    assert_eq!(
        h.app().answers,
        [("delete".to_string(), Answer::Button(0)), ("again".to_string(), Answer::Button(1))]
    );
}
