//! 自前で描くタイトルバーに並べる窓のボタン (− □ ✕ や、赤・黄・緑の丸)。
//!
//! ボタン 1 個は「名前・アイコン・押すと起きること」の組 ([`TitlebarButton`])。
//! 起きることは次のどちらか。
//!
//! * **窓の操作** ([`WindowAction`]: 最小化・拡大・フルスクリーン・閉じる) —
//!   ランタイムがクリックのその場で窓に掛ける。アプリは窓のハンドルも
//!   クリックの振り分けも持たなくていい
//! * **アプリの操作** (`action` が `None`) — 普通のボタンと同じく
//!   `on_click("{id}:{key}")` で受ける (ピン留め・設定を開く 等)
//!
//! ボタンの並びと見た目 ([`TitlebarButtonsStyle`]) を 1 つにしたのが
//! [`TitlebarButtons`]。よく使う形はプリセットになっている。
//!
//! | プリセット | 並び | 置き場所 |
//! |---|---|---|
//! | [`TitlebarButtons::windows`] | − □ ✕ (帯の高さいっぱいの四角) | 帯の右端 |
//! | [`TitlebarButtons::traffic_lights`] | 赤・黄・緑の丸 (閉じる・しまう・フルスクリーン) | 帯の左端 |
//!
//! 中身はただの `Vec` なので、足す・外す・並べ替える・アイコンや機能を
//! 差し替えるは自由。− □ ✕ も「3 つを右上に置いた」だけの 1 例にすぎない。
//!
//! ```ignore
//! fn titlebar(&self) -> Titlebar { Titlebar::Custom { height: 38.0 } }
//!
//! fn view(&self, ctx: &ViewContext) -> Element {
//!     let buttons = TitlebarButtons::windows(&ctx.theme, 38.0)
//!         // 先頭に「ピン留め」を足す。窓の操作ではないので on_click("win:pin") で受ける
//!         .insert(0, TitlebarButton::new("pin", TitlebarIcon::glyph("📌")))
//!         // ✕ の絵だけ差し替える (押すと閉じるのはそのまま)
//!         .edit("close", |b| b.icon(TitlebarIcon::svg("M2 2 L8 8 M8 2 L2 8", 10.0)));
//!     let bar = div().window_drag().h(Px(38.0)).flex_row().items_center().children([
//!         text("タイトル").ml(Px(12.0)),
//!         div().flex_1(),
//!         titlebar_buttons(ctx, "win", &buttons),
//!     ]);
//!     div().flex_col().children([bar, body])
//! }
//! ```
//!
//! 帯そのもの (高さ・`.window_drag()`・題や部品の並び) はアプリが描く。この部品が
//! 受け持つのはボタンの並びだけ。

use sabitori_core::element::{div, path, text, Dimension::Px, Element, Role, WindowAction};
use sabitori_core::{AppTheme, Color, ViewContext, WindowState};

/// ボタンの絵。組み込みの 4 つは線で描き、窓の状態で形が変わる
/// (拡大中の □ は「元に戻す」の重なった □ に、フルスクリーン中の四隅は内向きに)。
#[derive(Clone, Debug, PartialEq)]
pub enum TitlebarIcon {
    /// ─ (しまう)。
    Minimize,
    /// □ (拡大)。拡大中は重なった □ (元に戻す)。
    Maximize,
    /// 外向きの四隅 (フルスクリーンに入る)。フルスクリーン中は内向き (抜ける)。
    Fullscreen,
    /// ✕ (閉じる)。
    Close,
    /// 文字 1 つ (記号・絵文字)。大きさは [`TitlebarButtonsStyle::icon_size`] に合わせる。
    Glyph(String),
    /// SVG の `d` を線で描く。`view_box` は正方形の座標系の一辺 (Lucide なら 24)。
    /// 線の太さは座標系によらず [`TitlebarButtonsStyle::stroke_width`] px になる。
    Svg {
        /// SVG の `d` 属性。
        d: String,
        /// 座標系 `0 0 view_box view_box` の一辺。
        view_box: f32,
    },
}

impl TitlebarIcon {
    /// 文字 1 つの絵 ([`TitlebarIcon::Glyph`])。
    pub fn glyph(glyph: impl Into<String>) -> Self {
        Self::Glyph(glyph.into())
    }

    /// SVG の `d` を線で描く絵 ([`TitlebarIcon::Svg`])。
    pub fn svg(d: impl Into<String>, view_box: f32) -> Self {
        Self::Svg { d: d.into(), view_box }
    }
}

/// 組み込みの絵の線 (座標系は 10 × 10)。
const BUILTIN_BOX: f32 = 10.0;
const MINIMIZE: &str = "M0 5 H10";
const MAXIMIZE: &str = "M0.5 0.5 H9.5 V9.5 H0.5 Z";
const RESTORE: &str = "M2.5 2.5 V0.5 H9.5 V7.5 H7.5 M0.5 2.5 H7.5 V9.5 H0.5 Z";
const ENTER_FULLSCREEN: &str =
    "M0.5 3.5 V0.5 H3.5 M6.5 0.5 H9.5 V3.5 M9.5 6.5 V9.5 H6.5 M3.5 9.5 H0.5 V6.5";
const EXIT_FULLSCREEN: &str =
    "M3.5 0.5 V3.5 H0.5 M6.5 0.5 V3.5 H9.5 M9.5 6.5 H6.5 V9.5 M3.5 9.5 V6.5 H0.5";
const CLOSE: &str = "M0.5 0.5 L9.5 9.5 M9.5 0.5 L0.5 9.5";

/// 窓のボタン 1 個。
#[derive(Clone, Debug, PartialEq)]
pub struct TitlebarButton {
    /// 名前。要素の id は `"{titlebar_buttons の id}:{key}"` になり、押すと
    /// アプリの `on_click` にもその id が届く。
    pub key: String,
    /// 絵。
    pub icon: TitlebarIcon,
    /// 押すと窓に起きること。`None` なら窓には何もせず、アプリが `on_click` で受ける。
    pub action: Option<WindowAction>,
    /// 支援技術が読み上げる名前。
    pub label: String,
    /// 指が乗ったら地を [`TitlebarButtonsStyle::danger_bg`] にする (閉じる)。
    pub danger: bool,
    /// このボタンだけの地の色 (信号の丸)。`None` なら [`TitlebarButtonsStyle::bg`]。
    pub color: Option<Color>,
    /// フルスクリーン中は出さないか。`None` (既定) なら機能で決まる
    /// ([`Self::hides_in_fullscreen`])。
    pub hide_in_fullscreen: Option<bool>,
}

impl TitlebarButton {
    /// アプリの操作のボタン (押すと `on_click("{id}:{key}")`)。窓の操作にするなら
    /// [`Self::action`] を足す。
    pub fn new(key: impl Into<String>, icon: TitlebarIcon) -> Self {
        let key = key.into();
        Self {
            label: key.clone(),
            key,
            icon,
            action: None,
            danger: false,
            color: None,
            hide_in_fullscreen: None,
        }
    }

    /// フルスクリーン中に隠すか。決めていなければ ([`Self::hide_in_fullscreen`] が
    /// `None`) 機能で決まる:
    ///
    /// * 最小化・拡大・閉じるは **macOS でだけ**隠す — macOS のフルスクリーンでは OS の
    ///   信号ボタンが画面の上端に寄せると出る帯へ戻るので、二重になる。他の OS の
    ///   フルスクリーンには OS の帯が無いので、隠すと閉じる手段が消える
    /// * フルスクリーンは隠さない (抜ける手段なので)。アプリの操作も隠さない
    ///
    /// 機能で決まるので、□ をフルスクリーンに付け替えれば (`.action(..)`) フルスクリーン
    /// 中も残る。
    pub fn hides_in_fullscreen(&self) -> bool {
        self.hide_in_fullscreen.unwrap_or(
            cfg!(target_os = "macos")
                && matches!(
                    self.action,
                    Some(WindowAction::Minimize | WindowAction::ToggleMaximize | WindowAction::Close)
                ),
        )
    }

    /// しまう (`key` は `"minimize"`)。
    pub fn minimize() -> Self {
        Self::new("minimize", TitlebarIcon::Minimize)
            .action(WindowAction::Minimize)
            .label("最小化")
    }

    /// 拡大と元に戻すを行き来する (`key` は `"maximize"`)。
    pub fn maximize() -> Self {
        Self::new("maximize", TitlebarIcon::Maximize)
            .action(WindowAction::ToggleMaximize)
            .label("拡大")
    }

    /// フルスクリーンに入る / 抜ける (`key` は `"fullscreen"`)。抜ける手段なので、
    /// フルスクリーン中も出す。
    pub fn fullscreen() -> Self {
        Self::new("fullscreen", TitlebarIcon::Fullscreen)
            .action(WindowAction::ToggleFullscreen)
            .label("フルスクリーン")
    }

    /// 閉じる (`key` は `"close"`)。OS の閉じるボタンと同じ道を通る。
    pub fn close() -> Self {
        Self::new("close", TitlebarIcon::Close)
            .action(WindowAction::Close)
            .label("閉じる")
            .danger(true)
    }

    /// 押すと窓に起きることを決める。
    pub fn action(mut self, action: WindowAction) -> Self {
        self.action = Some(action);
        self
    }

    /// 窓の操作を外し、アプリが `on_click` で受けるボタンにする。
    pub fn app_action(mut self) -> Self {
        self.action = None;
        self
    }

    /// 絵を差し替える。
    pub fn icon(mut self, icon: TitlebarIcon) -> Self {
        self.icon = icon;
        self
    }

    /// 支援技術が読み上げる名前。
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// 指が乗ったら危険色にする (閉じる)。
    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    /// このボタンだけの地の色。
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// フルスクリーン中に隠すかを、機能によらず決める ([`Self::hides_in_fullscreen`])。
    pub fn hide_in_fullscreen(mut self, hide: bool) -> Self {
        self.hide_in_fullscreen = Some(hide);
        self
    }
}

/// ボタンの見た目。大きさ・間隔・角の丸み・色。
#[derive(Clone, Debug, PartialEq)]
pub struct TitlebarButtonsStyle {
    /// 1 個の幅 (論理 px)。
    pub width: f32,
    /// 1 個の高さ (論理 px)。帯いっぱいにするなら帯の高さ。
    pub height: f32,
    /// 角の丸み。幅・高さの半分で丸になる (それより大きくしない — 小さい箱では
    /// 何も描かれなくなる)。
    pub radius: f32,
    /// ボタンどうしの間隔。
    pub gap: f32,
    /// 絵の大きさ (正方形の一辺)。
    pub icon_size: f32,
    /// 線で描く絵の線の太さ (論理 px)。
    pub stroke_width: f32,
    /// 絵の色。
    pub icon: Color,
    /// 指が乗ったときの絵の色。
    pub icon_hover: Color,
    /// 地の色 ([`TitlebarButton::color`] が無いボタン)。
    pub bg: Color,
    /// 指が乗ったときの地の色。`None` なら変えない。
    pub hover_bg: Option<Color>,
    /// 危険なボタン ([`TitlebarButton::danger`]) に指が乗ったときの地の色。
    /// `None` なら [`Self::hover_bg`] と同じ扱い。
    pub danger_bg: Option<Color>,
    /// 危険なボタンに指が乗ったときの絵の色 ([`Self::danger_bg`] があるときだけ)。
    pub danger_icon: Color,
    /// 絵はどれかのボタンに指が乗っている間だけ出す (macOS の信号ボタンと同じ)。
    pub icons_on_hover: bool,
}

impl TitlebarButtonsStyle {
    /// 帯の高さいっぱいの四角に線の絵。指が乗ると地が浮き、閉じるは赤くなる
    /// (Windows の形)。色は `theme` から取る。
    pub fn windows(theme: &AppTheme, height: f32) -> Self {
        Self {
            width: 40.0,
            height,
            radius: 0.0,
            gap: 0.0,
            icon_size: 10.0,
            stroke_width: 1.0,
            icon: theme.text_secondary,
            icon_hover: theme.text_primary,
            bg: Color::TRANSPARENT,
            hover_bg: Some(theme.hover_bg),
            // Windows の閉じるボタンの赤。テーマの error は文字向けに暗いことがある。
            danger_bg: Some(Color::from_hex("#e81123")),
            danger_icon: Color::WHITE,
            icons_on_hover: false,
        }
    }

    /// 12px の丸を 8px 空けて並べ、絵は指が乗ったときだけ出す (macOS の形)。
    /// 丸の色は各ボタンの [`TitlebarButton::color`]。
    pub fn traffic_lights() -> Self {
        Self {
            width: 12.0,
            height: 12.0,
            radius: 6.0,
            gap: 8.0,
            icon_size: 6.0,
            stroke_width: 1.1,
            icon: Color::new(0.0, 0.0, 0.0, 0.6),
            icon_hover: Color::new(0.0, 0.0, 0.0, 0.6),
            bg: Color::from_hex("#8e8e93"),
            hover_bg: None,
            danger_bg: None,
            danger_icon: Color::new(0.0, 0.0, 0.0, 0.6),
            icons_on_hover: true,
        }
    }
}

/// 窓のボタンの並びと見た目 ([`titlebar_buttons`] に渡す)。
#[derive(Clone, Debug, PartialEq)]
pub struct TitlebarButtons {
    /// 左から順のボタン。
    pub buttons: Vec<TitlebarButton>,
    /// 見た目。
    pub style: TitlebarButtonsStyle,
}

impl TitlebarButtons {
    /// ボタン無しから組む。
    pub fn new(style: TitlebarButtonsStyle) -> Self {
        Self { buttons: Vec::new(), style }
    }

    /// − □ ✕ (最小化・拡大・閉じる)。帯の右端に置く。
    pub fn windows(theme: &AppTheme, height: f32) -> Self {
        Self::new(TitlebarButtonsStyle::windows(theme, height))
            .push(TitlebarButton::minimize())
            .push(TitlebarButton::maximize())
            .push(TitlebarButton::close())
    }

    /// 赤・黄・緑の丸 (閉じる・しまう・フルスクリーン)。帯の左端に置く。
    pub fn traffic_lights() -> Self {
        let mac = cfg!(target_os = "macos");
        Self::new(TitlebarButtonsStyle::traffic_lights())
            .push(TitlebarButton::close().color(Color::from_hex("#ff5f57")))
            .push(TitlebarButton::minimize().color(Color::from_hex("#febc2e")))
            // macOS のフルスクリーンでは OS の信号ボタンが戻るので、緑も一緒に隠す。
            .push(
                TitlebarButton::fullscreen()
                    .color(Color::from_hex("#28c840"))
                    .hide_in_fullscreen(mac),
            )
    }

    /// 末尾に足す。
    pub fn push(mut self, button: TitlebarButton) -> Self {
        self.buttons.push(button);
        self
    }

    /// `index` 番目に差し込む (長さを超えたら末尾)。
    pub fn insert(mut self, index: usize, button: TitlebarButton) -> Self {
        let index = index.min(self.buttons.len());
        self.buttons.insert(index, button);
        self
    }

    /// `key` のボタンを外す。
    pub fn remove(mut self, key: &str) -> Self {
        self.buttons.retain(|b| b.key != key);
        self
    }

    /// `key` のボタンを書き換える (絵・機能の差し替え)。無ければ何もしない。
    pub fn edit(mut self, key: &str, f: impl FnOnce(TitlebarButton) -> TitlebarButton) -> Self {
        if let Some(i) = self.buttons.iter().position(|b| b.key == key) {
            let button = self.buttons.remove(i);
            self.buttons.insert(i, f(button));
        }
        self
    }

    /// 見た目を差し替える。
    pub fn style(mut self, style: TitlebarButtonsStyle) -> Self {
        self.style = style;
        self
    }
}

/// [`titlebar_buttons`] が `key` のボタンに付ける id (`"{id}:{key}"`)。`on_click` で
/// 受けるときの突き合わせに。
pub fn titlebar_button_id(id: &str, key: &str) -> String {
    format!("{id}:{key}")
}

/// 窓のボタンを横に並べた要素。帯 (`.window_drag()` を付けた所) の中に置く —
/// ボタンは押せる物なので、上を押しても窓は動かない。
///
/// 各ボタンの id は `"{id}:{key}"` (`"win:close"` 等)。窓の操作のボタンは
/// ランタイムが窓に掛け、そうでないボタンはアプリが `on_click` で受ける。
/// フルスクリーン中は [`TitlebarButton::hides_in_fullscreen`] のボタンを出さない
/// (全部隠れたら大きさ 0 の要素になる)。
pub fn titlebar_buttons(ctx: &ViewContext, id: &str, buttons: &TitlebarButtons) -> Element {
    let style = &buttons.style;
    let state = ctx.window_state;
    let shown: Vec<(&TitlebarButton, String)> = buttons
        .buttons
        .iter()
        .filter(|b| !(state.fullscreen && b.hides_in_fullscreen()))
        .map(|b| (b, titlebar_button_id(id, &b.key)))
        .collect();
    let hovered = ctx.hovered.as_deref();
    let group_hot = shown.iter().any(|(_, bid)| hovered == Some(bid.as_str()));
    let children = shown.into_iter().map(|(b, bid)| {
        let hot = hovered == Some(bid.as_str());
        let base = b.color.unwrap_or(style.bg);
        let danger_bg = style.danger_bg.filter(|_| b.danger);
        let (bg, ink) = match (hot, danger_bg) {
            (false, _) => (base, style.icon),
            (true, Some(red)) => (red, style.danger_icon),
            (true, None) => (style.hover_bg.unwrap_or(base), style.icon_hover),
        };
        let mut el = div()
            .id(bid)
            .role(Role::Button)
            .label(b.label.clone())
            .w(Px(style.width))
            .h(Px(style.height))
            .shrink(0.0)
            .flex_row()
            .items_center()
            .justify_center()
            .bg(bg);
        if style.radius > 0.0 {
            el = el.rounded_px(style.radius);
        }
        if let Some(action) = b.action {
            el = el.window_action(action);
        }
        if !style.icons_on_hover || group_hot {
            el = el.child(icon_element(&b.icon, ink, style, state));
        }
        el
    });
    div()
        .no_select()
        .flex_row()
        .items_center()
        .shrink(0.0)
        .gap(style.gap)
        .children(children)
}

/// 絵 1 つ。線の絵は `icon_size` 四方に引き伸ばし、線の太さは `stroke_width` px に保つ。
fn icon_element(icon: &TitlebarIcon, color: Color, style: &TitlebarButtonsStyle, state: WindowState) -> Element {
    let size = style.icon_size;
    let (d, view_box) = match icon {
        TitlebarIcon::Minimize => (MINIMIZE, BUILTIN_BOX),
        TitlebarIcon::Maximize if state.maximized => (RESTORE, BUILTIN_BOX),
        TitlebarIcon::Maximize => (MAXIMIZE, BUILTIN_BOX),
        TitlebarIcon::Fullscreen if state.fullscreen => (EXIT_FULLSCREEN, BUILTIN_BOX),
        TitlebarIcon::Fullscreen => (ENTER_FULLSCREEN, BUILTIN_BOX),
        TitlebarIcon::Close => (CLOSE, BUILTIN_BOX),
        TitlebarIcon::Glyph(glyph) => {
            return text(glyph.clone()).font_size((size * 1.2).max(8.0)).color(color);
        }
        TitlebarIcon::Svg { d, view_box } => (d.as_str(), view_box.max(f32::EPSILON)),
    };
    path()
        .svg_path(d)
        .view_box(0.0, 0.0, view_box, view_box)
        .w(Px(size))
        .h(Px(size))
        .shrink(0.0)
        // 座標系ごと引き伸ばすので、線の太さは座標系の単位で渡す。
        .stroke(color, style.stroke_width * view_box / size.max(f32::EPSILON))
}
