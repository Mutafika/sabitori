//! **左にナビ + 右に本文**の枠組み。窓の幅に合わせて形を変える
//! ([#98](https://github.com/Mutafika/sabitori/issues/98))。
//!
//! 業務アプリはほぼ全部この形をしている。サイドバーを固定幅のまま置くと、
//! 窓を狭めた (ブラウザを半分にする・iPad を縦にする・画面を左右に分ける) ときに
//! 本文から先に苦しくなる — 820px の窓で 210px のサイドバーが残ると、本文は
//! 600px を切る。
//!
//! | 幅 ([`SizeClass`]) | 形 ([`NavMode`]) |
//! |---|---|
//! | Expanded (≥ 1040) | 固定幅のサイドバー |
//! | Medium (640〜1040) | アイコン + 短い名前の細い列 (Material の NavigationRail) |
//! | Compact (< 640) | 上のバーのボタンで開く引き出し (本文の上に重なる) |
//!
//! アプリはナビの**中身** ([`NavGroup`] / [`NavItem`]) と選択中の項目だけを渡す。
//! 形の切り替え・引き出しの開閉のばね・「選んだら閉じる」は枠組みが持つ。
//!
//! ```ignore
//! struct App { page: String, nav: NavFrameState }
//!
//! fn view(&self, ctx: &ViewContext) -> Element {
//!     let groups = [
//!         NavGroup::new("業務")
//!             .item(NavItem::new("dispatch", "配車表").icon("▦").short("配車"))
//!             .item(NavItem::new("vehicles", "車両一覧").icon("◎").short("車両")),
//!         NavGroup::new("請求").item(NavItem::new("invoices", "請求書").icon("¥")),
//!     ];
//!     nav_frame(
//!         ctx, "nav", &self.nav, &NavFrameStyle::from_theme(&ctx.theme),
//!         &groups, &self.page,
//!         |app: &mut App, id| app.page = id.to_string(),
//!         self.page_view(ctx),
//!     )
//! }
//! ```
//!
//! 引き出しは**選ばれている項目が変わったら閉じる**。項目を押したときだけでなく、
//! 本文の中のリンクや「戻る」で画面が移った場合も閉じる (URL が変わったら閉じる)。

use std::cell::RefCell;
use std::rc::Rc;

use sabitori_anim::{Animated, Spring};
use sabitori_core::element::{div, text, Percent, Px, Role};
use sabitori_core::{Color, Element, Managed, SizeClass, ViewContext};

/// ナビの 1 項目。
#[derive(Clone, Debug, PartialEq)]
pub struct NavItem {
    /// 選択の照合と、押されたときに `on_select` へ渡す値。
    pub id: String,
    /// サイドバー・引き出しに出る名前。上のバーの見出しにもなる。
    pub label: String,
    /// 細い列で名前の上に出す 1〜2 文字 (記号・絵文字)。無ければ名前の先頭 1 文字。
    pub icon: Option<String>,
    /// 細い列で出す短い名前。無ければ `label`。
    pub short: Option<String>,
}

impl NavItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into(), icon: None, short: None }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn short(mut self, short: impl Into<String>) -> Self {
        self.short = Some(short.into());
        self
    }

    fn icon_text(&self) -> String {
        self.icon
            .clone()
            .unwrap_or_else(|| self.label.chars().next().map(String::from).unwrap_or_default())
    }

    fn short_text(&self) -> &str {
        self.short.as_deref().unwrap_or(&self.label)
    }
}

/// 見出し付きの項目のまとまり (`業務` / `請求` / `レポート` …)。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavGroup {
    /// 見出し。空なら出さない。細い列では見出しの代わりに区切り線を引く。
    pub title: String,
    pub items: Vec<NavItem>,
}

impl NavGroup {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), items: Vec::new() }
    }

    pub fn item(mut self, item: NavItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = NavItem>) -> Self {
        self.items.extend(items);
        self
    }
}

/// いまの形。[`nav_frame`] が窓の幅から選ぶ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMode {
    /// 固定幅のサイドバー。
    Sidebar,
    /// アイコン + 短い名前の細い列。
    Rail,
    /// 上のバーのボタンで開く引き出し。
    Drawer,
}

impl NavMode {
    /// 幅から形を選ぶ。区切りは [`SizeClass`] と同じ。
    pub fn for_width(width: f32) -> Self {
        match SizeClass::from_width(width) {
            SizeClass::Expanded => NavMode::Sidebar,
            SizeClass::Medium => NavMode::Rail,
            SizeClass::Compact => NavMode::Drawer,
        }
    }
}

/// 見た目。
#[derive(Clone, Debug)]
pub struct NavFrameStyle {
    pub sidebar_width: f32,
    pub rail_width: f32,
    /// 引き出しの幅。窓がこれより狭ければ、右に 56px 残して縮む。
    pub drawer_width: f32,
    /// 引き出しの形で上に出るバーの高さ。
    pub bar_height: f32,
    pub nav_bg: Color,
    pub content_bg: Color,
    pub border: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub accent: Color,
    pub selected_bg: Color,
    pub hover_bg: Color,
    /// 引き出しが開いている間、本文に被せる幕。
    pub scrim: Color,
}

impl NavFrameStyle {
    /// [`AppTheme`](sabitori_core::AppTheme) から組む。寸法は `default_dark()` と同じ。
    pub fn from_theme(theme: &sabitori_core::AppTheme) -> Self {
        Self {
            nav_bg: theme.surface,
            content_bg: theme.bg,
            border: theme.border,
            text: theme.text_primary,
            text_secondary: theme.text_secondary,
            accent: theme.primary,
            selected_bg: theme.select_bg,
            hover_bg: theme.hover_bg,
            ..Self::default_dark()
        }
    }

    pub fn default_dark() -> Self {
        let t = sabitori_core::AppTheme::midnight();
        Self {
            sidebar_width: 210.0,
            rail_width: 76.0,
            drawer_width: 280.0,
            bar_height: 48.0,
            nav_bg: t.surface,
            content_bg: t.bg,
            border: t.border,
            text: t.text_primary,
            text_secondary: t.text_secondary,
            accent: t.primary,
            selected_bg: t.select_bg,
            hover_bg: t.hover_bg,
            scrim: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }
}

struct NavInner {
    drawer_open: bool,
    anim: Animated<f32>,
    /// 前のフレームで選ばれていた項目。変わったら引き出しを閉じる。
    last_selected: Option<String>,
}

/// 引き出しの開閉。`App` のフィールドに置く。`Rc` のハンドルなので `clone()` は安い。
///
/// サイドバー・細い列の形では使わない (開閉が無い)。
#[derive(Clone)]
pub struct NavFrameState(Rc<RefCell<NavInner>>);

impl Managed for NavFrameState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// 開閉のばねを進める。**アプリが `tick` を書く必要は無い。**
    fn advance(&self, dt: f32) {
        self.0.borrow_mut().anim.tick(dt);
    }

    fn animating(&self) -> bool {
        self.0.borrow().anim.running
    }
}

impl Default for NavFrameState {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for NavFrameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NavFrameState")
            .field("drawer_open", &self.is_drawer_open())
            .field("progress", &self.progress())
            .finish()
    }
}

impl NavFrameState {
    /// 引き出しを閉じた状態で作る。
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(NavInner {
            drawer_open: false,
            anim: Animated::new(0.0).with_spring(Spring::snappy()),
            last_selected: None,
        })))
    }

    pub fn open_drawer(&self) {
        let mut i = self.0.borrow_mut();
        i.drawer_open = true;
        i.anim.set_target(1.0);
    }

    pub fn close_drawer(&self) {
        let mut i = self.0.borrow_mut();
        i.drawer_open = false;
        i.anim.set_target(0.0);
    }

    pub fn toggle_drawer(&self) {
        if self.is_drawer_open() {
            self.close_drawer();
        } else {
            self.open_drawer();
        }
    }

    /// 引き出しが開いている (閉じかけを含まない)。
    pub fn is_drawer_open(&self) -> bool {
        self.0.borrow().drawer_open
    }

    /// 引き出しの開き具合 (0.0 = 閉、1.0 = 開)。
    pub fn progress(&self) -> f32 {
        self.0.borrow().anim.value()
    }

    /// 選ばれている項目を見て、変わっていれば引き出しを閉じる。
    fn note_selected(&self, selected: &str) {
        let changed = {
            let mut i = self.0.borrow_mut();
            let changed = i.last_selected.as_deref().is_some_and(|s| s != selected);
            i.last_selected = Some(selected.to_string());
            changed
        };
        if changed {
            self.close_drawer();
        }
    }
}

/// 一覧・細い列の上下の余白。セーフエリアはこれに足す (上書きすると消える)。
const LIST_PAD: f32 = 8.0;

/// 項目を押したときにアプリへ渡す口。
type OnSelect<A> = Rc<dyn Fn(&mut A, &str)>;

/// 項目の要素 id。`on_select` を使わずに `DeclarativeApp::on_click` で受ける
/// 場合や、テストで押す場合に。
pub fn nav_item_id(id: &str, item: &str) -> String {
    format!("{id}::item::{item}")
}

/// 引き出しを開くボタンの要素 id。
pub fn nav_menu_button_id(id: &str) -> String {
    format!("{id}::menu")
}

/// ナビ + 本文を組む。**窓いっぱいに置く** (`view()` の根か、`w_full().h_full()` の直下)。
///
/// * 形は `ctx.width` から選ぶ ([`NavMode::for_width`])
/// * 項目を押すと `on_select(app, 項目の id)` が呼ばれる
/// * 引き出しは、選ばれている項目が変わったら閉じる。幕を押しても閉じる
/// * ナビの側は `ctx.safe_area` の内側に置く (iOS のステータスバー等)。
///   本文の側はアプリが持つ
#[allow(clippy::too_many_arguments)]
pub fn nav_frame<A: 'static>(
    ctx: &ViewContext,
    id: &str,
    state: &NavFrameState,
    style: &NavFrameStyle,
    groups: &[NavGroup],
    selected: &str,
    on_select: impl Fn(&mut A, &str) + 'static,
    content: Element,
) -> Element {
    ctx.register_managed(id, Rc::new(state.clone()));
    state.note_selected(selected);

    let mode = NavMode::for_width(ctx.width);
    // 広げたら引き出しは要らない。開いたまま残すと、また狭めたときに急に被さる。
    if mode != NavMode::Drawer && state.is_drawer_open() {
        state.close_drawer();
    }

    let on_select: OnSelect<A> = Rc::new(on_select);
    let safe = ctx.safe_area;
    let body = div()
        .id(format!("{id}::content"))
        .grow(1.0)
        .min_w(Px(0.0))
        .min_h(Px(0.0))
        .h_full()
        .bg(style.content_bg)
        .flex_col()
        .child(content);

    match mode {
        NavMode::Sidebar | NavMode::Rail => {
            let nav = if mode == NavMode::Sidebar {
                list(ctx, id, style, groups, selected, &on_select, None)
                    .w(Px(style.sidebar_width + safe.left))
            } else {
                rail(ctx, id, style, groups, selected, &on_select).w(Px(style.rail_width + safe.left))
            };
            div()
                .id(id)
                .w_full()
                .h_full()
                .flex_row()
                .child(
                    nav.shrink(0.0)
                        .h_full()
                        .pt(Px(LIST_PAD + safe.top))
                        .pl(Px(safe.left))
                        .pb(Px(LIST_PAD + safe.bottom))
                        .bg(style.nav_bg),
                )
                .child(div().w(Px(1.0)).h_full().shrink(0.0).bg(style.border))
                .child(body)
        }
        NavMode::Drawer => {
            let title = groups
                .iter()
                .flat_map(|g| g.items.iter())
                .find(|i| i.id == selected)
                .map(|i| i.label.clone())
                .unwrap_or_default();
            let menu_id = nav_menu_button_id(id);
            {
                let state = state.clone();
                ctx.register_action(
                    menu_id.clone(),
                    Rc::new(move |_app: &mut dyn std::any::Any| state.toggle_drawer()),
                );
            }
            let bar = div()
                .id(format!("{id}::bar"))
                .w_full()
                .h(Px(style.bar_height + safe.top))
                .pt(Px(safe.top))
                .pl(Px(safe.left))
                .pr(Px(safe.right))
                .shrink(0.0)
                .flex_row()
                .items_center()
                .gap(4.0)
                .bg(style.nav_bg)
                .child(
                    div()
                        .id(&menu_id)
                        .role(Role::Button)
                        .label("メニュー")
                        .w(Px(style.bar_height))
                        .h(Px(style.bar_height))
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(4.0)
                        .hover(|s| s.bg(style.hover_bg))
                        // 3 本線は字 (☰) ではなく矩形で描く。web に同梱の書体には
                        // この字が無く、豆腐になっていた。
                        .children((0..3).map(|_| {
                            div().w(Px(18.0)).h(Px(2.0)).shrink(0.0).rounded_px(1.0).bg(style.text)
                        })),
                )
                .child(
                    text(&title)
                        .font_size(16.0)
                        .bold()
                        .color(style.text)
                        .max_lines(1)
                        .min_w(Px(0.0)),
                );
            let mut root = div()
                .id(id)
                .w_full()
                .h_full()
                .flex_col()
                .child(bar)
                .child(div().w_full().h(Px(1.0)).shrink(0.0).bg(style.border))
                .child(body);
            let progress = state.progress();
            if progress > 0.01 || state.is_drawer_open() {
                root = root.child(drawer(ctx, id, state, style, groups, selected, &on_select, progress));
            }
            root
        }
    }
}

/// 引き出し (幕 + 左から出るパネル)。`.overlay()` で本文より手前に出す。
#[allow(clippy::too_many_arguments)]
fn drawer<A: 'static>(
    ctx: &ViewContext,
    id: &str,
    state: &NavFrameState,
    style: &NavFrameStyle,
    groups: &[NavGroup],
    selected: &str,
    on_select: &OnSelect<A>,
    progress: f32,
) -> Element {
    let scrim_id = format!("{id}::scrim");
    let panel_id = format!("{id}::drawer");
    {
        let state = state.clone();
        ctx.register_action(
            scrim_id.clone(),
            Rc::new(move |_app: &mut dyn std::any::Any| state.close_drawer()),
        );
    }
    // パネル自身にも id を付けてクリックを吸わせる (余白を押して幕が閉じないように)。
    ctx.register_action(panel_id.clone(), Rc::new(|_app: &mut dyn std::any::Any| {}));

    let safe = ctx.safe_area;
    let width = style.drawer_width.min((ctx.width - 56.0).max(0.0)) + safe.left;
    let panel = list(ctx, id, style, groups, selected, on_select, Some(state))
        .id(&panel_id)
        .role(Role::Dialog)
        .label("メニュー")
        .w(Px(width))
        .h_full()
        .shrink(0.0)
        .pt(Px(LIST_PAD + safe.top))
        .pl(Px(safe.left))
        .pb(Px(LIST_PAD + safe.bottom))
        .bg(style.nav_bg)
        .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
        .tx(-width * (1.0 - progress));
    div()
        .id(&scrim_id)
        .overlay()
        .pos(0.0, 0.0)
        .w(Percent(100.0))
        .h(Percent(100.0))
        .bg(Color::new(style.scrim.r, style.scrim.g, style.scrim.b, style.scrim.a * progress))
        .flex_row()
        .child(panel)
}

/// サイドバー / 引き出しの中身: 見出し付きの一覧。
#[allow(clippy::too_many_arguments)]
fn list<A: 'static>(
    ctx: &ViewContext,
    id: &str,
    style: &NavFrameStyle,
    groups: &[NavGroup],
    selected: &str,
    on_select: &OnSelect<A>,
    closes: Option<&NavFrameState>,
) -> Element {
    let mut col = div()
        .id(format!("{id}::list"))
        .scroll(format!("{id}::list"))
        .flex_col()
        .py(Px(LIST_PAD));
    for group in groups {
        if !group.title.is_empty() {
            col = col.child(
                text(&group.title)
                    .font_size(11.0)
                    .bold()
                    .color(style.text_secondary)
                    .px_pad(Px(16.0))
                    .pt(Px(12.0))
                    .pb(Px(4.0))
                    .shrink(0.0),
            );
        }
        for item in &group.items {
            let on = item.id == selected;
            let row = div()
                .role(Role::Link)
                .label(&item.label)
                .mx(Px(8.0))
                .px_pad(Px(8.0))
                .h(Px(34.0))
                .shrink(0.0)
                .flex_row()
                .items_center()
                .gap(10.0)
                .rounded_px(6.0)
                .bg(if on { style.selected_bg } else { Color::TRANSPARENT })
                .hover(|s| s.bg(if on { style.selected_bg } else { style.hover_bg }))
                .children([
                    text(item.icon_text())
                        .font_size(14.0)
                        .color(if on { style.accent } else { style.text_secondary })
                        .w(Px(20.0))
                        .shrink(0.0),
                    bold_if(text(&item.label), on)
                        .font_size(14.0)
                        .color(if on { style.accent } else { style.text })
                        .max_lines(1)
                        .min_w(Px(0.0)),
                ]);
            col = col.child(item_click(ctx, id, item, on_select, closes, row));
        }
    }
    col
}

/// 細い列: アイコンの下に短い名前。見出しの代わりに区切り線。
fn rail<A: 'static>(
    ctx: &ViewContext,
    id: &str,
    style: &NavFrameStyle,
    groups: &[NavGroup],
    selected: &str,
    on_select: &OnSelect<A>,
) -> Element {
    let mut col = div()
        .id(format!("{id}::rail"))
        .scroll(format!("{id}::rail"))
        .flex_col()
        .items_center()
        .py(Px(LIST_PAD))
        .gap(2.0);
    for (gi, group) in groups.iter().enumerate() {
        if gi > 0 {
            col = col.child(div().w(Px(32.0)).h(Px(1.0)).my(Px(6.0)).bg(style.border).shrink(0.0));
        }
        for item in &group.items {
            let on = item.id == selected;
            let cell = div()
                .role(Role::Link)
                .label(&item.label)
                .tooltip(&item.label)
                .w(Px(style.rail_width - 12.0))
                .py(Px(6.0))
                .shrink(0.0)
                .flex_col()
                .items_center()
                .gap(2.0)
                .rounded_px(8.0)
                .bg(if on { style.selected_bg } else { Color::TRANSPARENT })
                .hover(|s| s.bg(if on { style.selected_bg } else { style.hover_bg }))
                .children([
                    text(item.icon_text())
                        .font_size(18.0)
                        .color(if on { style.accent } else { style.text_secondary }),
                    bold_if(text(item.short_text()), on)
                        .font_size(10.0)
                        .color(if on { style.accent } else { style.text_secondary })
                        .max_lines(1)
                        .min_w(Px(0.0)),
                ]);
            col = col.child(item_click(ctx, id, item, on_select, None, cell));
        }
    }
    col
}

fn bold_if(el: Element, on: bool) -> Element {
    if on { el.bold() } else { el }
}

/// 項目に押したときの動きを付ける。引き出しの中なら閉じもする。
fn item_click<A: 'static>(
    ctx: &ViewContext,
    id: &str,
    item: &NavItem,
    on_select: &OnSelect<A>,
    closes: Option<&NavFrameState>,
    el: Element,
) -> Element {
    let on_select = on_select.clone();
    let item_id = item.id.clone();
    let closes = closes.cloned();
    el.click(ctx, nav_item_id(id, &item.id), move |app: &mut A| {
        on_select(app, &item_id);
        if let Some(state) = &closes {
            state.close_drawer();
        }
    })
}
