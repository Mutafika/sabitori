use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Element, ScrollbarStyle};
use sabitori_core::element::{div, text, Auto, Percent, Px, Role};
use sabitori_core::{Managed, ViewContext};
use std::cell::RefCell;
use std::rc::Rc;

/// Style for modal dialog.
#[derive(Clone, Debug)]
pub struct ModalStyle {
    pub backdrop_color: Color,
    pub bg: Color,
    pub border_color: Color,
    /// 見出しの文字の色 (`modal(.., title, ..)`)。
    pub title_color: Color,
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
            title_color: theme.text_primary,
            scrollbar: Some(ScrollbarStyle::from_theme(theme)),
            ..Self::default_dark()
        }
    }

    pub fn default_dark() -> Self {
        Self {
            backdrop_color: Color::new(0.0, 0.0, 0.0, 0.63),
            bg: Color::from_hex("#1e1e2e"),
            border_color: Color::from_hex("#3a3a55"),
            title_color: Color::from_hex("#e8e8f0"),
            corner_radius: 12.0,
            shadow_blur: 32.0,
            max_width: 500.0,
            max_height: 400.0,
            padding: 24.0,
            scrollbar: Some(ScrollbarStyle::default_dark()),
        }
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
/// 0.58 までは、アプリが `tick(dt)` を回し高さが `max_height` に固定の `Modal`
/// 構造体もあった (テストの `settle` で閉じきらず、中にフォームを置くと下が切れた —
/// [#75](https://github.com/Mutafika/sabitori/issues/75) の 7・15・16)。使う所が
/// 無くなったので 0.59 で消した。高さは**中身なり**で、上限だけ `max_height`。
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
                .color(style.title_color)
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
        // 帯はダイアログの右の余白に置く。枠の右端に置くと、掴める幅 (14px) が
        // `w_full` の欄の右端に被り、欄を押したつもりが帯を掴む。枠だけを余白の
        // ぶん右へ広げ、同じだけ内側に詰めるので、中身の幅は変わらない。
        body = body
            .w(Auto)
            .mr(Px(-style.padding))
            .pr(Px(style.padding))
            .scrollbar_style(bar);
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
