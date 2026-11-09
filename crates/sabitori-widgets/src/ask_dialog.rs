//! 確かめのダイアログを窓の中に描く。
//!
//! 問い・補足・ボタンを [`Ask`] で渡すと、窓の中央に出し、押されたボタンの番号を
//! [`AskDialogState::take_answers`] で返す。OS の確かめの窓 (macOS の `NSAlert`) と
//! 同じ振る舞いにしてある:
//!
//! - 1 つ目のボタンが既定。右端に置き、Return で押される
//! - [`Ask::cancel_button`] で足したボタンは Esc で押される
//! - 背景を押しても閉じない (どれかのボタンで答える)
//! - 出ている間、キーは全部ダイアログが受ける ([`AskDialogState::on_key`])
//!
//! **ふつうは `sabitori::dialog::ask` から使う。** ランタイムがこの部品を窓の上に
//! 重ね、キーを回し、答えを `on_dialog_answered` に届けるので、アプリは何も配線
//! しなくてよい。自分で重ねたいときだけ、`overlay_view` でこの部品を直に使う:
//!
//! ```ignore
//! // 持つ
//! asks: AskDialogState,
//!
//! // 聞く
//! self.asks.ask("delete", Ask::new("3 個の項目をゴミ箱に入れますか？")
//!     .button("ゴミ箱に入れる").cancel_button("キャンセル"));
//!
//! // overlay_view
//! ask_dialog(ctx, "ask", &self.asks, &AskDialogStyle::from_theme(&ctx.theme))
//!
//! // on_input (キーを先に渡す)
//! if self.asks.is_open() { if let InputEvent::KeyInput { key, pressed: true, .. } = event { self.asks.on_key(*key); } return true; }
//!
//! // 答え (on_click の後など、毎フレーム)
//! for (key, button) in self.asks.take_answers() { /* … */ }
//! ```

use crate::modal::ModalState;
use sabitori_core::element::{div, text, Cursor, Dimension::Px, Element, Percent, Role};
use sabitori_core::{AppTheme, Color, ViewContext};
use sabitori_input::Key;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

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

    /// 出すボタンの文字。[`Self::buttons`] が空なら「OK」1 つ (答えは `0`)。
    pub fn labels(&self) -> Vec<&str> {
        if self.buttons.is_empty() {
            vec!["OK"]
        } else {
            self.buttons.iter().map(String::as_str).collect()
        }
    }
}

/// [`ask_dialog`] の見た目。
#[derive(Clone, Debug)]
pub struct AskDialogStyle {
    /// 窓全体に掛ける幕の色。
    pub backdrop: Color,
    pub bg: Color,
    pub border: Color,
    pub corner_radius: f32,
    /// ダイアログの幅 (窓が狭ければ窓に合わせて縮む)。
    pub width: f32,
    pub padding: f32,
    /// 問い (1 行目)。
    pub message: Color,
    /// 補足。
    pub detail: Color,
    /// Return / Esc の案内。
    pub hint: Color,
    /// 案内を出すか。
    pub show_key_hints: bool,
    /// 既定のボタン (1 つ目) の地・指が乗ったときの地・文字。
    pub primary_bg: Color,
    pub primary_bg_hover: Color,
    pub primary_text: Color,
    /// ほかのボタンの地・指が乗ったときの地・文字・枠。
    pub button_bg: Color,
    pub button_bg_hover: Color,
    pub button_text: Color,
    pub button_border: Color,
    pub button_radius: f32,
    /// 文字を等幅で組む (端末風の見た目)。
    pub mono: bool,
}

impl AskDialogStyle {
    /// 色は `theme` から、寸法は既定のまま。`ctx.theme` をそのまま渡せる。
    pub fn from_theme(theme: &AppTheme) -> Self {
        Self {
            backdrop: Color::new(0.0, 0.0, 0.0, 0.45),
            bg: theme.elevated,
            border: theme.border,
            corner_radius: 12.0,
            width: 400.0,
            padding: 22.0,
            message: theme.text_primary,
            detail: theme.text_secondary,
            hint: theme.text_secondary.with_alpha(0.7),
            show_key_hints: true,
            primary_bg: theme.primary,
            primary_bg_hover: theme.primary.with_alpha(0.85),
            primary_text: Color::WHITE,
            button_bg: Color::TRANSPARENT,
            button_bg_hover: theme.hover_bg,
            button_text: theme.text_primary,
            button_border: theme.border,
            button_radius: 7.0,
            mono: false,
        }
    }
}

/// 出している問いと、届いた答え。`Rc` のハンドルなので `clone()` は安い。
#[derive(Clone, Default)]
pub struct AskDialogState(Rc<RefCell<Inner>>);

#[derive(Default)]
struct Inner {
    /// 描いている問い。閉じても消えていく間は描くので、次の問いまで残す。
    shown: Option<(String, Ask)>,
    /// 出している間に来た問い。答えると次を出す。
    queue: VecDeque<(String, Ask)>,
    /// 押された答え (札とボタンの番号)。
    answers: Vec<(String, usize)>,
    modal: ModalState,
}

impl std::fmt::Debug for AskDialogState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let i = self.0.borrow();
        f.debug_struct("AskDialogState")
            .field("shown", &i.shown)
            .field("open", &i.modal.is_open())
            .field("queued", &i.queue.len())
            .finish()
    }
}

impl AskDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 問いを出す。答えは `key` の札付きで [`Self::take_answers`] に来る。
    /// 出している最中なら、今の問いに答えた後に出す。
    pub fn ask(&self, key: impl Into<String>, ask: Ask) {
        let mut i = self.0.borrow_mut();
        let question = (key.into(), ask);
        if i.modal.is_open() {
            i.queue.push_back(question);
        } else {
            i.shown = Some(question);
            i.modal.open();
        }
    }

    /// 問いを出している (消えていく間は含まない)。
    pub fn is_open(&self) -> bool {
        self.0.borrow().modal.is_open()
    }

    /// 出している問い (札と中身)。
    pub fn current(&self) -> Option<(String, Ask)> {
        let i = self.0.borrow();
        if i.modal.is_open() { i.shown.clone() } else { None }
    }

    /// `button` 番目のボタンを押す。出していなければ何もしない。
    pub fn press(&self, button: usize) {
        let mut i = self.0.borrow_mut();
        if !i.modal.is_open() {
            return;
        }
        let Some((key, ask)) = i.shown.clone() else { return };
        let button = button.min(ask.labels().len() - 1);
        i.answers.push((key, button));
        match i.queue.pop_front() {
            // 次の問いは閉じずにそのまま差し替える。
            Some(next) => i.shown = Some(next),
            None => i.modal.close(),
        }
    }

    /// 出している間のキー。Return は既定 (1 つ目)、Esc はキャンセルのボタンを押す。
    /// 出している間は `true` (ほかのキーも、下の画面に渡さず吸う)。
    pub fn on_key(&self, key: Key) -> bool {
        if !self.is_open() {
            return false;
        }
        match key {
            Key::Enter => self.press(0),
            Key::Escape => {
                let cancel = self.0.borrow().shown.as_ref().and_then(|(_, ask)| ask.cancel);
                if let Some(cancel) = cancel {
                    self.press(cancel);
                }
            }
            _ => {}
        }
        true
    }

    /// 届いた答え (札とボタンの番号) を、押された順に取り出す。
    pub fn take_answers(&self) -> Vec<(String, usize)> {
        std::mem::take(&mut self.0.borrow_mut().answers)
    }

    fn modal(&self) -> ModalState {
        self.0.borrow().modal.clone()
    }
}

/// 問いを窓の中に描く。何も出していなければ (閉じきっていれば) `None`。
///
/// 窓いっぱいの幕を `absolute` で左上から敷くので、`overlay_view` のどこに
/// 置いてもよい。ボタンの id は `"{id}::button::{n}"`、箱は `"{id}::dialog"`。
pub fn ask_dialog(
    ctx: &ViewContext,
    id: &str,
    state: &AskDialogState,
    style: &AskDialogStyle,
) -> Option<Element> {
    // 閉じきっていても登録はする (ばねを回すのはランタイム。切れると閉じる
    // アニメーションが途中で止まる)。
    let modal = state.modal();
    ctx.register_managed(format!("{id}::modal"), Rc::new(modal.clone()));
    if modal.is_fully_closed() {
        return None;
    }
    let (_, ask) = state.0.borrow().shown.clone()?;
    let progress = modal.progress();
    let open = modal.is_open();

    let label = |s: &str, size: f32, weight: u16, color: Color| -> Element {
        let t = text(s).font_size(size).font_weight(weight).color(color);
        if style.mono { t.mono() } else { t }
    };

    // 既定 (1 つ目) を右端に。右から 1 つ目・2 つ目… (macOS の並び)。
    let labels = ask.labels();
    let buttons: Vec<Element> = labels
        .iter()
        .enumerate()
        .rev()
        .map(|(n, name)| {
            let button_id = format!("{id}::button::{n}");
            let hot = ctx.hovered.as_deref() == Some(button_id.as_str());
            let primary = n == 0;
            let (bg, fg) = match (primary, hot) {
                (true, false) => (style.primary_bg, style.primary_text),
                (true, true) => (style.primary_bg_hover, style.primary_text),
                (false, false) => (style.button_bg, style.button_text),
                (false, true) => (style.button_bg_hover, style.button_text),
            };
            let mut button = div()
                .cursor(Cursor::Pointer)
                .shrink(0.0)
                .bg(bg)
                .rounded_px(style.button_radius)
                .px_pad(Px(14.0))
                .py(Px(6.0))
                .role(Role::Button)
                .label(*name)
                .child(label(name, 13.5, if primary { 600 } else { 500 }, fg).shrink(0.0));
            if !primary {
                button = button.border(1.0, style.button_border);
            }
            // 押せるのは開いている間だけ (消えていく箱が次のクリックを吸わないように)。
            if open {
                let state = state.clone();
                ctx.register_action(button_id.clone(), Rc::new(move |_app: &mut dyn std::any::Any| state.press(n)));
                button = button.id(button_id);
            }
            button
        })
        .collect();

    let mut body = vec![label(&ask.message, 15.0, 700, style.message).shrink(0.0)];
    if !ask.detail.is_empty() {
        body.push(label(&ask.detail, 13.0, 400, style.detail).shrink(0.0));
    }
    body.push(div().h(Px(4.0)));
    body.push(div().w_full().flex_row().justify_end().gap(8.0).children(buttons));
    if style.show_key_hints {
        let mut hint = format!("Return: {}", labels[0]);
        if let Some(cancel) = ask.cancel.and_then(|c| labels.get(c)) {
            hint.push_str(&format!("  ·  Esc: {cancel}"));
        }
        body.push(label(&hint, 11.5, 400, style.hint).self_end());
    }

    let backdrop_id = format!("{id}::backdrop");
    let dialog_id = format!("{id}::dialog");
    // 幕も箱もクリックを吸う。幕を押しても閉じない (OS の確かめの窓と同じ)。
    if open {
        let noop: sabitori_core::Action = Rc::new(|_app: &mut dyn std::any::Any| {});
        ctx.register_action(backdrop_id.clone(), noop.clone());
        ctx.register_action(dialog_id.clone(), noop);
    }
    let mut dialog = div()
        .role(Role::Dialog)
        .label(ask.message.as_str())
        .w(Px(style.width))
        .max_w(Percent(100.0))
        .bg(style.bg)
        .border(1.0, style.border)
        .rounded_px(style.corner_radius)
        .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5 * progress))
        .opacity(progress)
        .scaled(0.96 + 0.04 * progress)
        .p(Px(style.padding))
        .flex_col()
        .gap(12.0)
        .children(body);
    let mut backdrop = div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(ctx.width))
        .h(Px(ctx.height))
        .bg(style.backdrop.with_alpha(style.backdrop.a * progress))
        .flex_col()
        .items_center()
        .justify_center();
    if open {
        dialog = dialog.id(dialog_id);
        backdrop = backdrop.id(backdrop_id);
    }
    Some(backdrop.child(dialog))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save() -> Ask {
        Ask::new("保存しますか？").button("保存").button("保存しない").cancel_button("キャンセル")
    }

    #[test]
    fn builder_marks_the_cancel_button() {
        let a = save();
        assert_eq!(a.buttons, ["保存", "保存しない", "キャンセル"]);
        assert_eq!(a.cancel, Some(2));
        assert_eq!(Ask::new("終わりました").labels(), ["OK"]);
    }

    #[test]
    fn keys_press_the_default_and_the_cancel_button() {
        let s = AskDialogState::new();
        assert!(!s.on_key(Key::Enter), "出していなければ吸わない");
        s.ask("a", save());
        assert!(s.on_key(Key::J), "出している間は何でも吸う");
        assert!(s.take_answers().is_empty());
        s.on_key(Key::Escape);
        assert_eq!(s.take_answers(), [("a".to_string(), 2)]);
        assert!(!s.is_open());

        s.ask("b", save());
        s.on_key(Key::Enter);
        assert_eq!(s.take_answers(), [("b".to_string(), 0)]);
    }

    #[test]
    fn esc_does_nothing_without_a_cancel_button() {
        let s = AskDialogState::new();
        s.ask("a", Ask::new("消しました").button("OK"));
        s.on_key(Key::Escape);
        assert!(s.is_open(), "キャンセルが無ければ Esc では閉じない");
        assert!(s.take_answers().is_empty());
    }

    #[test]
    fn questions_wait_their_turn() {
        let s = AskDialogState::new();
        s.ask("first", save());
        s.ask("second", Ask::new("次").button("OK"));
        assert_eq!(s.current().map(|(k, _)| k).as_deref(), Some("first"));
        s.press(1);
        assert_eq!(s.current().map(|(k, _)| k).as_deref(), Some("second"), "答えたら次が出る");
        s.press(0);
        assert!(!s.is_open());
        assert_eq!(s.take_answers(), [("first".to_string(), 1), ("second".to_string(), 0)]);
    }

    #[test]
    fn a_button_past_the_end_counts_as_the_last() {
        let s = AskDialogState::new();
        s.ask("a", Ask::new("x"));
        s.press(5);
        assert_eq!(s.take_answers(), [("a".to_string(), 0)]);
    }
}
