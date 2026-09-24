use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Element, Rect, ScrollbarStyle};
use sabitori_core::element::{div, text, Percent, Px, Role};
use sabitori_core::{Managed, ViewContext};
use std::cell::RefCell;
use std::rc::Rc;

/// Style for modal dialog.
#[derive(Clone, Debug)]
pub struct ModalStyle {
    pub backdrop_color: Color,
    pub bg: Color,
    pub border_color: Color,
    pub corner_radius: f32,
    pub shadow_blur: f32,
    pub max_width: f32,
    pub max_height: f32,
    pub padding: f32,
    /// 中身が `max_height` に当たってスクロールするときの帯 ([#90])。
    /// `None` なら出さない。
    ///
    /// [#90]: https://github.com/Mutafika/sabitori/issues/90
    pub scrollbar: Option<ScrollbarStyle>,
}

impl ModalStyle {
    /// [`AppTheme`] から組む。
    ///
    /// **色はテーマから、寸法は `default_dark()` と同じ**。テーマを差し替えても
    /// ウィジェットが追従しないのが [#65] の中身で、既定が全部「いつもの紫」に
    /// なっていた。`ctx.theme` をそのまま渡せる:
    ///
    /// ```ignore
    /// modal(ctx, "id", &state, &ModalStyle::from_theme(&ctx.theme))
    /// ```
    ///
    /// [`AppTheme`]: sabitori_core::AppTheme
    /// [#65]: https://github.com/Mutafika/sabitori/issues/65
    pub fn from_theme(theme: &sabitori_core::AppTheme) -> Self {
        Self {
            bg: theme.elevated,
            border_color: theme.border,
            scrollbar: Some(ScrollbarStyle::from_theme(theme)),
            ..Self::default_dark()
        }
    }

    pub fn default_dark() -> Self {
        Self {
            backdrop_color: Color::new(0.0, 0.0, 0.0, 0.63),
            bg: Color::from_hex("#1e1e2e"),
            border_color: Color::from_hex("#3a3a55"),
            corner_radius: 12.0,
            shadow_blur: 32.0,
            max_width: 500.0,
            max_height: 400.0,
            padding: 24.0,
            scrollbar: Some(ScrollbarStyle::default_dark()),
        }
    }
}

/// Modal dialog overlay.
pub struct Modal {
    pub visible: bool,
    pub title: String,
    pub style: ModalStyle,
    /// Viewport size for centering.
    pub viewport: Rect,
    /// Open/close animation (0=closed, 1=open).
    pub open_anim: Animated<f32>,
}

impl Modal {
    pub fn new(title: &str, style: ModalStyle) -> Self {
        Self {
            visible: false,
            title: title.to_string(),
            style,
            viewport: Rect::new(0.0, 0.0, 800.0, 600.0),
            open_anim: Animated::new(0.0).with_spring(Spring::snappy()),
        }
    }

    /// Open the modal.
    pub fn open(&mut self) {
        self.visible = true;
        self.open_anim.set_target(1.0);
    }

    /// Close the modal.
    pub fn close(&mut self) {
        self.visible = false;
        self.open_anim.set_target(0.0);
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    /// Set viewport size (for centering).
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = Rect::new(0.0, 0.0, width, height);
    }

    /// The backdrop rect (full viewport).
    pub fn backdrop_rect(&self) -> Rect {
        self.viewport
    }

    /// Backdrop opacity (animated).
    pub fn backdrop_opacity(&self) -> f32 {
        self.open_anim.value()
    }

    /// The modal dialog rect (centered, animated scale).
    pub fn dialog_rect(&self) -> Rect {
        let scale = self.open_anim.value();
        let w = self.style.max_width * scale;
        let h = self.style.max_height * scale;
        let x = (self.viewport.size.width - w) / 2.0;
        let y = (self.viewport.size.height - h) / 2.0;
        Rect::new(x, y, w, h)
    }

    /// The content area inside the dialog (with padding).
    pub fn content_rect(&self) -> Rect {
        let d = self.dialog_rect();
        let p = self.style.padding;
        Rect::new(
            d.origin.x + p,
            d.origin.y + p,
            d.size.width - p * 2.0,
            d.size.height - p * 2.0,
        )
    }

    /// Whether animation is complete (for cleanup).
    pub fn is_fully_closed(&self) -> bool {
        !self.visible && self.open_anim.value() < 0.01
    }

    /// Whether the modal is open (or animating to open).
    pub fn is_open(&self) -> bool {
        self.visible
    }

    /// Current animation progress (0.0 = fully closed, 1.0 = fully open).
    pub fn progress(&self) -> f32 {
        self.open_anim.value()
    }

    /// Tick animations.
    pub fn tick(&mut self, dt: f32) {
        self.open_anim.tick(dt);
    }

    /// Build a complete overlay Element for this modal.
    ///
    /// Returns `None` if the modal is fully closed (not visible and animation done).
    ///
    /// * `viewport_w`, `viewport_h` — viewport dimensions for backdrop sizing.
    /// * `backdrop_id` — element ID for the backdrop (for click-to-dismiss).
    /// * `dialog_w` — desired width of the dialog.
    /// * `bg` — dialog background color.
    /// * `border` — dialog border color.
    /// * `content` — child elements to place inside the dialog.
    pub fn to_overlay(
        &self,
        viewport_w: f32,
        viewport_h: f32,
        backdrop_id: &str,
        dialog_w: f32,
        bg: Color,
        border: Color,
        content: Vec<Element>,
    ) -> Option<Element> {
        if !self.is_open() && self.progress() <= 0.01 {
            return None;
        }

        let progress = self.progress();
        let backdrop_alpha = progress * 0.63;
        let padding = self.style.padding;

        // Center the dialog horizontally and vertically
        let left = (viewport_w - dialog_w) / 2.0;
        let dialog_h = self.style.max_height;
        let top = (viewport_h - dialog_h) / 2.0;

        // Dialog panel
        let dialog = div()
            .role(Role::Dialog)
            .label(&self.title)
            .pos(left, top)
            .w(Px(dialog_w))
            .h(Px(dialog_h))
            .bg(bg)
            .border(1.0, border)
            .rounded_px(self.style.corner_radius)
            .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
            .opacity(progress)
            .p(Px(padding))
            .flex_col()
            .overflow_hidden()
            .children(content);

        // Backdrop + dialog
        let overlay = div()
            .id(backdrop_id)
            .w(Px(viewport_w))
            .h(Px(viewport_h))
            .pos(0.0, 0.0)
            .bg(Color::new(0.0, 0.0, 0.0, backdrop_alpha))
            .overlay()
            .child(dialog);

        Some(overlay)
    }
}

// ---------------------------------------------------------------------------
// ランタイムが面倒を見るモーダル (#75 の 7 / 15 / 16)
// ---------------------------------------------------------------------------

struct ModalInner {
    open: bool,
    dismissable: bool,
    anim: Animated<f32>,
}

/// **開閉のばねをランタイムが回すモーダル。**
///
/// [`Modal`] との違いは 3 つ、どれも業務画面のフォームを載せると効いてくる
/// ([#75](https://github.com/Mutafika/sabitori/issues/75) の 7・15・16):
///
/// | | [`Modal`] | `ModalState` |
/// |---|---|---|
/// | 開閉の tick | アプリが `tick(dt)` を書く | ランタイムが回す |
/// | 高さ | `max_height` に固定 | **中身なり**、上限だけ `max_height` |
/// | テストの `settle` | 終わらない (閉じかけの背景が次のクリックを吸う) | 終わる |
///
/// 高さが固定だったのが一番効いていて、中にフォームを置くと下が切れるか
/// スカスカになるので、アプリは「一覧の上に出すページ内フォーム」を書いていた。
///
/// # 使い方
///
/// ```ignore
/// // App のフィールド
/// edit: ModalState,
///
/// // view() — 配線はこれだけ
/// let overlay = modal(ctx, "edit", &self.edit, &ModalStyle::from_theme(&ctx.theme), "予約を編集", vec![
///     form_rows(ctx, self),
/// ]);
/// div().w_full().h_full().children(page).children(overlay)
///
/// // 開く / 閉じる
/// button("編集").click(ctx, "open-edit", |app: &mut App| app.edit.open())
/// ```
///
/// `Rc` のハンドルなので `clone()` は安い。
#[derive(Clone)]
pub struct ModalState(Rc<RefCell<ModalInner>>);

impl Managed for ModalState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// 開閉のばねを進める。**アプリが `tick` を書く必要は無い。**
    fn advance(&self, dt: f32) {
        self.0.borrow_mut().anim.tick(dt);
    }

    /// 開閉の最中だけ `true`。落ち着いたら下りるので `settle` は終わる。
    fn animating(&self) -> bool {
        self.0.borrow().anim.running
    }
}

impl Default for ModalState {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ModalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModalState")
            .field("open", &self.is_open())
            .field("progress", &self.progress())
            .finish()
    }
}

impl ModalState {
    /// 閉じた状態で作る。
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(ModalInner {
            open: false,
            dismissable: true,
            anim: Animated::new(0.0).with_spring(Spring::snappy()),
        })))
    }

    /// 開く。
    pub fn open(&self) {
        let mut i = self.0.borrow_mut();
        i.open = true;
        i.anim.set_target(1.0);
    }

    /// 閉じる (アニメーションしながら消える)。
    pub fn close(&self) {
        let mut i = self.0.borrow_mut();
        i.open = false;
        i.anim.set_target(0.0);
    }

    pub fn toggle(&self) {
        if self.is_open() {
            self.close();
        } else {
            self.open();
        }
    }

    /// 開いている (閉じかけを含まない)。
    pub fn is_open(&self) -> bool {
        self.0.borrow().open
    }

    /// 開閉の進み具合 (0.0 = 閉、1.0 = 開)。
    pub fn progress(&self) -> f32 {
        self.0.borrow().anim.value()
    }

    /// 背景を押したら閉じるか (既定 `true`)。
    ///
    /// 保存前に消えては困るフォームでは `false` にする。押しても閉じないが、
    /// **背景は今までどおりクリックを吸う** — 下の一覧が反応するのは
    /// 「閉じていないのに操作できる」なので、そちらのほうが困る。
    pub fn set_dismissable(&self, yes: bool) {
        self.0.borrow_mut().dismissable = yes;
    }

    pub fn is_dismissable(&self) -> bool {
        self.0.borrow().dismissable
    }

    /// 完全に閉じきった (描くものが無い)。
    pub fn is_fully_closed(&self) -> bool {
        let i = self.0.borrow();
        !i.open && i.anim.value() < 0.01
    }
}

/// [`ModalState`] を画面に出す。閉じきっていれば `None`。
///
/// * 中央に出る (座標計算なし — 背景が中央寄せのフレックス)
/// * 高さは**中身なり**で、`style.max_height` を超えたら中身がスクロールする
/// * 背景を押すと閉じる ([`ModalState::set_dismissable`] で止められる)
/// * 開閉のばねはランタイムが回す
///
/// `title` が空なら見出し行は出ない (支援技術向けの名前も付かないので、
/// 見出しを自分で組むなら [`Element::label`] を足すこと)。
///
/// **`div().w_full().h_full()` の直下に置くこと。** 背景は親の 100% を取る。
pub fn modal(
    ctx: &ViewContext,
    id: &str,
    state: &ModalState,
    style: &ModalStyle,
    title: &str,
    content: Vec<Element>,
) -> Option<Element> {
    // 閉じきっていても**登録はする**。ばねを進めるのはランタイムなので、
    // 登録が切れると閉じるアニメーションが最後の 1 フレームで止まる。
    ctx.register_managed(id, Rc::new(state.clone()));

    if state.is_fully_closed() {
        return None;
    }

    let progress = state.progress();
    let backdrop_id = format!("{id}::backdrop");
    let dialog_id = format!("{id}::dialog");

    // 背景のクリックで閉じる。アプリの型を知らなくてよいので、`Element::click`
    // ではなく口に直接積む (状態は `Rc` の中にあり、アプリに触らない)。
    {
        let state = state.clone();
        ctx.register_action(
            backdrop_id.clone(),
            Rc::new(move |_app: &mut dyn std::any::Any| {
                if state.is_dismissable() {
                    state.close();
                }
            }),
        );
    }
    // ダイアログ自身にも id を付けて**クリックを吸わせる**。付けないと、
    // 余白や見出しを押しただけで下の背景が拾って閉じる。
    ctx.register_action(dialog_id.clone(), Rc::new(|_app: &mut dyn std::any::Any| {}));

    let mut dialog = div()
        .id(&dialog_id)
        .role(Role::Dialog)
        .w(Px(style.max_width))
        .max_w(Percent(100.0))
        .max_h(Px(style.max_height))
        .bg(style.bg)
        .border(1.0, style.border_color)
        .rounded_px(style.corner_radius)
        .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
        .opacity(progress)
        // 開くときだけ少し大きくなる。閉じきりでは 0.96 から。
        .scaled(0.96 + 0.04 * progress)
        .p(Px(style.padding))
        .gap(12.0)
        .flex_col();
    if !title.is_empty() {
        dialog = dialog.label(title).child(
            text(title)
                .font_size(16.0)
                .bold()
                .color(Color::from_hex("#e8e8f0"))
                .shrink(0.0),
        );
    }

    // 中身。上限に当たったときだけスクロールする。
    //
    // `.scroll()` を書くと flex item の automatic minimum size が 0 になるので、
    // ここが上限より小さくなれる (書かないと中身の高さのまま押し広げて、
    // ダイアログが `max_height` を超える)。
    let mut body = div()
        .id(format!("{id}::body"))
        .scroll(format!("{id}::body"))
        .w_full()
        .flex_col()
        .gap(12.0)
        .children(content);
    if let Some(bar) = &style.scrollbar {
        body = body.scrollbar_style(bar);
    }
    dialog = dialog.child(body);

    Some(
        div()
            .id(&backdrop_id)
            .overlay()
            .w(Percent(100.0))
            .h(Percent(100.0))
            .bg(Color::new(0.0, 0.0, 0.0, style.backdrop_color.a * progress))
            .items_center()
            .justify_center()
            .child(dialog),
    )
}
