//! Declarative UI element builders.
//!
//! Provides a builder-pattern API for constructing UI trees:
//!
//! ```ignore
//! div()
//!     .w(Px(200.0)).h_full()
//!     .bg(theme.surface)
//!     .rounded(Px(8.0))
//!     .p(Px(16.0)).gap(8.0)
//!     .flex_col()
//!     .children([
//!         text("Hello").font_size(24.0).color(theme.text_primary),
//!         button("Click").accent(theme.primary).on_click(|| log("clicked")),
//!     ])
//! ```

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::{Color, Corners, Point};

// ---------------------------------------------------------------------------
// Dimension (local to avoid circular dep with sabitori-style)
// ---------------------------------------------------------------------------

/// CSS-like dimension value for the element builder API.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
}

impl Default for Dimension {
    fn default() -> Self {
        Self::Auto
    }
}

/// Shorthand constructors.
impl Dimension {
    pub fn px(v: f32) -> Self {
        Self::Px(v)
    }

    pub fn pct(v: f32) -> Self {
        Self::Percent(v)
    }
}

/// Convenience: `200.0.px()` and `50.0.pct()`.
pub trait DimensionExt {
    fn px(self) -> Dimension;
    fn pct(self) -> Dimension;
}

impl DimensionExt for f32 {
    fn px(self) -> Dimension {
        Dimension::Px(self)
    }
    fn pct(self) -> Dimension {
        Dimension::Percent(self)
    }
}

impl DimensionExt for i32 {
    fn px(self) -> Dimension {
        Dimension::Px(self as f32)
    }
    fn pct(self) -> Dimension {
        Dimension::Percent(self as f32)
    }
}

// Allow `Px(200.0)` shorthand at call site.
pub use Dimension::Auto;
pub use Dimension::Percent;
pub use Dimension::Px;

// ---------------------------------------------------------------------------
// Enums for layout properties
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignItems {
    #[default]
    Stretch,
    Start,
    End,
    Center,
    /// 文字のベースラインを揃える。 フォントサイズの違う文字を横に並べたとき、
    /// `Center` は箱の中心を合わせるので大きい方の文字が沈んで見える。
    Baseline,
}

/// 子 1 個だけが親の [`AlignItems`] から外れる (CSS `align-self`)。
///
/// 親の指定を全員に効かせたうえで 1 個だけ例外にしたい、 が CSS では日常だが
/// sabitori には無かったので、 その子を別の入れ物で包んで逃がすしかなかった。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignSelf {
    /// 親の [`AlignItems`] に従う (既定)。
    #[default]
    Auto,
    Stretch,
    Start,
    End,
    Center,
    Baseline,
}

/// 交差軸方向に**行そのもの**をどう配るか (CSS `align-content`)。
///
/// [`FlexWrap::Wrap`] で複数行になったとき、 または grid の行に効く。 折り返しが
/// 起きない限り意味を持たない ([`AlignItems`] と紛らわしいのはここ — あちらは
/// 「1 行の中で子をどこに置くか」)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignContent {
    #[default]
    Start,
    End,
    Center,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// 折り返した行の揃え方 (CSS `text-align`)。
///
/// **要素の箱の中で行がどう並ぶか**であって、 箱そのものの位置ではない。
/// 1 行しかないテキストを画面の中央に置きたいなら、 これではなく親の
/// [`JustifyContent::Center`] を使う。 これが効くのは折り返しが起きた後の
/// 2 行目以降を含む「行の揃え」。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    /// 書字方向の先頭側 (既定)。 日本語 / 英語なら左。
    #[default]
    Start,
    Center,
    /// 書字方向の末尾側。 日本語 / 英語なら右。
    End,
    /// 両端揃え。 最終行は先頭側のまま (CSS と同じ)。
    Justify,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum JustifyContent {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

/// この要素が子をどう並べるか。
///
/// CSS の `display` のうち、 アプリの UI で要るのはこの 2 つだけ。 `none` は
/// 入れていない — 宣言的に組む以上「隠す」は要素を**出さない**ことで書けて、
/// そちらの方が中身の計算ごと消える。
///
/// ```ignore
/// if self.show_sidebar { children.push(sidebar) }   // display: none の代わり
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    /// フレックスボックス。 既定。
    #[default]
    Flex,
    /// グリッド。 [`Element::grid_cols`] / [`Element::grid_rows`] で線を引く。
    Grid,
}

// ---------------------------------------------------------------------------
// Grid
// ---------------------------------------------------------------------------

/// トラックの下限 / 上限に置ける大きさ。 単体で使うことは少なく、 普通は
/// [`Track`] のコンストラクタ越しに指定する。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum TrackSize {
    /// 固定 px。
    Px(f32),
    /// グリッド全体に対する割合 (0–100)。
    Pct(f32),
    /// 余った空間の分配比 (CSS の `fr`)。 **上限側にしか置けない** —
    /// 下限に書いた場合は `Auto` として扱う (CSS と同じ制約)。
    Fr(f32),
    /// 中身に合わせる。
    #[default]
    Auto,
    /// 折り返せるだけ折り返したときの幅。
    MinContent,
    /// 一切折り返さないときの幅。
    MaxContent,
}

/// グリッドのトラック (列 1 本 / 行 1 本) の大きさ。
///
/// 中身は CSS の `minmax(min, max)` そのままで、 下限と上限の対。
/// `Track::px(200.0)` のような単一指定は「下限も上限も 200px」に展開される。
///
/// ```ignore
/// // サイドバー固定 + 本文が余りを取る
/// .grid_cols([Track::px(240.0), Track::fr(1.0)])
///
/// // 3 等分
/// .grid_cols(Track::repeat(3, Track::fr(1.0)))
///
/// // 最低 120px を保証しつつ余りを分ける (カードの並べ方の定番)
/// .grid_cols(Track::repeat(4, Track::minmax(Track::px(120.0), Track::fr(1.0))))
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub min: TrackSize,
    pub max: TrackSize,
}

impl Track {
    /// 固定 px。
    pub fn px(v: f32) -> Self {
        Self::both(TrackSize::Px(v))
    }

    /// グリッド全体に対する割合 (0–100)。
    pub fn pct(v: f32) -> Self {
        Self::both(TrackSize::Pct(v))
    }

    /// 余りを分け合う (CSS の `1fr` = `Track::fr(1.0)`)。
    ///
    /// 下限は `0` ではなく `Auto` — CSS の `fr` と同じで、 **中身より小さくは
    /// ならない**。 中身を無視して等分したいなら
    /// `Track::minmax(Track::px(0.0), Track::fr(1.0))`。
    pub fn fr(v: f32) -> Self {
        Self { min: TrackSize::Auto, max: TrackSize::Fr(v) }
    }

    /// 中身に合わせる。
    pub fn auto() -> Self {
        Self::both(TrackSize::Auto)
    }

    /// 折り返せるだけ折り返したときの幅。
    pub fn min_content() -> Self {
        Self::both(TrackSize::MinContent)
    }

    /// 一切折り返さないときの幅。
    pub fn max_content() -> Self {
        Self::both(TrackSize::MaxContent)
    }

    /// `minmax(min, max)`。 `min` からは下限が、 `max` からは上限が採られる。
    pub fn minmax(min: Track, max: Track) -> Self {
        Self { min: min.min, max: max.max }
    }

    /// 同じトラックを `n` 本。 CSS の `repeat(n, ..)` に当たるが、 本数は
    /// 呼び出し側が決める (`auto-fill` / `auto-fit` は未対応)。
    pub fn repeat(n: usize, track: Track) -> Vec<Track> {
        vec![track; n]
    }

    fn both(s: TrackSize) -> Self {
        Self { min: s, max: s }
    }
}

/// グリッド内での位置。 [`Element::col`] / [`Element::row`] に渡す。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridPlacement {
    /// 自動配置 (既定)。
    #[default]
    Auto,
    /// 指定した**線**に接する。 1 始まりで、 負の値は末尾から数える
    /// (`-1` = 最後の線)。 CSS の `grid-column-start` と同じ数え方。
    Line(i16),
    /// トラックを `n` 本ぶんまたぐ。
    Span(u16),
}

/// 自動配置の進み方 (CSS `grid-auto-flow`)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridAutoFlow {
    /// 行を左から埋めて、 足りなくなったら行を足す (既定)。
    #[default]
    Row,
    /// 列を上から埋めて、 足りなくなったら列を足す。
    Column,
    /// `Row` + 隙間詰め。 後ろの小さい要素が前の穴に入る。
    RowDense,
    /// `Column` + 隙間詰め。
    ColumnDense,
}

// ---------------------------------------------------------------------------
// Edge dimensions (padding / margin)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgeDimensions {
    pub top: Dimension,
    pub right: Dimension,
    pub bottom: Dimension,
    pub left: Dimension,
}

impl EdgeDimensions {
    pub fn all(v: Dimension) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }

    pub fn axes(vertical: Dimension, horizontal: Dimension) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
}

// ---------------------------------------------------------------------------
// Box shadow
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Point,
    pub blur: f32,
    pub spread: f32,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            offset: Point::ZERO,
            blur: 0.0,
            spread: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Image types
// ---------------------------------------------------------------------------

/// Raw decoded image data. Uses Arc for cheap cloning in view() calls.
#[derive(Clone, Debug)]
pub struct ImageData {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

impl ImageData {
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self { rgba: Arc::new(rgba), width, height }
    }
}

/// How an image fills its container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    /// Scale to fill, cropping if necessary. Preserves aspect ratio.
    #[default]
    Cover,
    /// Scale to fit entirely within the box. Preserves aspect ratio.
    Contain,
    /// Stretch to fill (ignores aspect ratio).
    Fill,
}

// ---------------------------------------------------------------------------
// ElementStyle — resolved style for a single element
// ---------------------------------------------------------------------------

/// Per-range background highlight for a text element — Word-style "find in
/// page" highlight. `ranges` are `(byte_start, byte_end)` offsets into the
/// text's content; the runtime paints a background rect behind the glyphs of
/// each range, drawn below the text and above the element background. It reuses
/// the same glyph-hitbox layout the text selection system uses, so highlights
/// track wrapping and CJK shaping exactly. `current` (an index into `ranges`)
/// is painted in `current_color` instead of `color` — used to accent the
/// active match in a find bar. Set via `Element::highlight`.
#[derive(Clone, Debug)]
pub struct HighlightSpec {
    /// Byte ranges `(start, end)` into the text content to highlight.
    pub ranges: Vec<(usize, usize)>,
    /// Background color for every range except `current`.
    pub color: Color,
    /// Index into `ranges` of the active match, or `None`.
    pub current: Option<usize>,
    /// Background color for the `current` match.
    pub current_color: Color,
}

impl Default for HighlightSpec {
    fn default() -> Self {
        Self {
            ranges: Vec::new(),
            color: Color::TRANSPARENT,
            current: None,
            current_color: Color::TRANSPARENT,
        }
    }
}

/// One clickable + hoverable byte range inside a text element's content.
/// Resolved to on-screen rects through the same glyph-hitbox layout the text
/// selection / [`HighlightSpec`] systems use, so it tracks wrapping and CJK
/// shaping exactly. A pointer press inside the range dispatches
/// `DeclarativeApp::on_click(id)`; `tooltip` (if set) shows on hover; the range
/// is underlined in `color`. Set via `Element::link_ranges` — used for in-body
/// 条文リンク (citation links) in flowing text.
#[derive(Clone, Debug)]
pub struct LinkRange {
    /// Byte offset range `[start, end)` into the (untruncated) text content.
    pub start: usize,
    pub end: usize,
    /// Dispatched via `DeclarativeApp::on_click` when the range is clicked.
    pub id: String,
    /// Hover-preview text (shown through the tooltip system). `None` = no tooltip.
    pub tooltip: Option<String>,
    /// Underline color for the range.
    pub color: Color,
}

/// 支援技術に伝える、 この要素の役割。
///
/// GPU で全部自前描画している以上、 OS から見ると窓の中身はただのピクセルで、
/// VoiceOver / NVDA / Narrator からは**完全に空の窓**に見える (issue #21)。
/// ネイティブウィジェットを使うツールキットと違い、 「何もしなければそこそこ動く」
/// という逃げ道が無い。 役割とラベルを構造として書けるようにするのが第一歩。
///
/// 値は ARIA / accesskit の role にほぼ対応する。 迷ったら [`Role::Group`]
/// (意味を持たない入れ物) にしておくこと — 嘘の役割は無いより悪い。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Role {
    /// 意味を持たない入れ物。 レイアウトのためだけの `div` はこれ。
    #[default]
    Group,
    /// 押すと何かが起きるもの。
    Button,
    /// 別の場所へ移動するもの。
    Link,
    /// 単一行のテキスト入力。
    TextInput,
    /// 複数行のテキスト入力。
    TextArea,
    /// on/off の切り替え。
    Checkbox,
    /// 排他選択の 1 つ。
    Radio,
    /// 値を連続的に変えるつまみ。
    Slider,
    /// 選択肢を開くもの。
    ComboBox,
    /// 見出し。 階層は [`Element::heading_level`] で示す。
    Heading,
    /// 読み上げ対象の本文テキスト。
    Text,
    /// 画像。 内容は [`Element::label`] で説明する (alt テキスト相当)。
    Image,
    /// 一覧の入れ物。
    List,
    /// 一覧の 1 項目。
    ListItem,
    /// タブの並び。
    TabList,
    /// タブ 1 枚。
    Tab,
    /// モーダルなどの前面領域。
    Dialog,
    /// 進捗表示。
    ProgressBar,
    /// 区切り線。
    Separator,
    /// 行と列を持つ表。 中身は [`Role::Row`] → [`Role::Cell`] の入れ子。
    Table,
    /// 表の 1 行。
    Row,
    /// 表のセル。
    Cell,
    /// 表の列見出しセル。
    ColumnHeader,
    /// 入れ子の木構造の入れ物。
    Tree,
    /// 木の 1 項目。 深さは [`Element::heading_level`] に入れる (1 が根)。
    TreeItem,
}

/// スクロール位置を誰が持つか。 `overflow` が [`Overflow::Scroll`] のときだけ意味がある。
///
/// sabitori のスクロールには最初から 2 つのモデルがあったが、 データ上は区別が無く、
/// ランタイムは `Overflow::Scroll` の要素を**全部**管理対象にしていた。 そのため
/// アプリが自分で持っているつもりのオフセットが毎フレーム上書きされ、
/// **手動モードが事実上存在しなかった** (issue #14)。 その区別をここで明示する。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollOwner {
    /// ランタイムが位置を持つ。 要素の `id` がその状態のキーで、 ホイール・慣性・
    /// バウンスはランタイムが面倒を見る。 [`Element::scroll`] が設定する。
    ///
    /// **`id` が無い要素は管理対象にならない** (キーが無いので状態を引けない)。
    /// `.scroll(id)` は id を必ず取るので、 その道を通れば起こらない。
    #[default]
    Runtime,
    /// アプリが位置を持つ。 ランタイムは `scroll_x` / `scroll_y` に触れないので、
    /// ホイールは `on_scroll_xy` などで自分で受けて値を進める。
    /// [`Element::scroll_manual`] が設定する。
    App,
}

/// Complete resolved style for one element.
#[derive(Clone, Debug)]
pub struct ElementStyle {
    // Layout
    pub position: Position,
    /// フレックスか grid か。 [`Display::Flex`] が既定。
    pub display: Display,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    /// この子だけ親の `align_items` から外れる。 [`AlignSelf::Auto`] (既定) は
    /// 「親に従う」。
    pub align_self: AlignSelf,
    /// grid で、 セルの中の主軸方向の寄せ。 flex では無視される。
    pub justify_self: AlignSelf,
    /// 折り返した**行**そのものの配り方。 折り返しか grid でだけ意味がある。
    /// `None` (既定) は taffy の既定に任せる。
    pub align_content: Option<AlignContent>,
    /// grid で、 セルの中の主軸方向の寄せの既定値 (子の `justify_self` が優先)。
    pub justify_items: Option<AlignItems>,
    /// 幅 ÷ 高さ。 片方の辺だけ決めれば、 もう片方がこの比で決まる。
    /// `None` (既定) = 比の拘束なし。
    pub aspect_ratio: Option<f32>,
    /// grid の列。 空 (既定) なら列は自動生成される。
    pub grid_template_columns: Vec<Track>,
    /// grid の行。 空 (既定) なら行は自動生成される。
    pub grid_template_rows: Vec<Track>,
    /// 自動配置の進み方。
    pub grid_auto_flow: GridAutoFlow,
    /// この要素が置かれる列 (開始, 終了)。
    pub grid_column: (GridPlacement, GridPlacement),
    /// この要素が置かれる行 (開始, 終了)。
    pub grid_row: (GridPlacement, GridPlacement),
    /// 兄弟の中での重なり順。 大きいほど手前。 既定は `0`。
    ///
    /// **同じ親を持つ兄弟の中でしか効かない** (CSS の重なり文脈と同じ)。
    /// 親を飛び越えて最前面に出したいなら [`Element::overlay`] を使うこと。
    pub z_index: i32,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    /// Flex base size along the parent's main axis. `Auto` (default) sizes
    /// from the width/height property, falling back to content size — the
    /// CSS `flex-basis: auto` behavior. `flex_1()` sets this to `Px(0.0)`
    /// (Tailwind `flex-1` = `flex: 1 1 0%`) so growing items split the
    /// *free* space instead of starting from their content size.
    pub flex_basis: Dimension,
    pub gap: f32,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub min_height: Dimension,
    pub max_width: Dimension,
    pub max_height: Dimension,
    pub padding: EdgeDimensions,
    pub margin: EdgeDimensions,
    pub overflow: Overflow,
    /// Inset for absolute positioning (top, right, bottom, left).
    pub inset_top: Dimension,
    pub inset_right: Dimension,
    pub inset_bottom: Dimension,
    pub inset_left: Dimension,
    /// Scroll offset in logical pixels (x, y). Applied when overflow is Scroll.
    pub scroll_x: f32,
    pub scroll_y: f32,
    /// スクロール位置を誰が持つか。 [`Overflow::Scroll`] のときだけ意味がある。
    pub scroll_owner: ScrollOwner,

    // Visual
    pub background: Color,
    /// Gradient end color. When set with gradient_angle != 0, creates a linear gradient.
    pub gradient_end: Color,
    /// Gradient angle in radians (0 = left-to-right, PI/2 = top-to-bottom).
    pub gradient_angle: f32,
    pub corner_radius: Corners<f32>,
    pub border_color: Color,
    pub border_width: f32,
    pub shadow: Option<BoxShadow>,
    pub opacity: f32,
    pub object_fit: ObjectFit,
    /// 回転角 (ラジアン)。正 = 画面上時計回り (Y 下向き座標系)。
    ///
    /// 矩形は**中心**まわり (線描画 = 回転した細 rect 用)、テキストは
    /// **原点** (レイアウト後の左上) まわりに回る。CAD の注記が挿入点まわりに
    /// 回る仕様に合わせたため。背景付きの要素を回すと箱とラベルはずれる。
    pub rotation: f32,
    /// Visual-only X offset, applied after layout. Does not push siblings —
    /// taffy still measures the element at its laid-out slot, but the rect
    /// (and its hit region) is rendered shifted by this many px. Drives
    /// hover-spring "lift / pull" effects without disturbing flex flow.
    pub translate_x: f32,
    /// Visual-only Y offset. See `translate_x` for semantics.
    pub translate_y: f32,
    /// Visual-only uniform scale about the element's **center**, applied after
    /// layout and inherited by the whole subtree — the CSS `transform: scale()`
    /// model. `1.0` (default) = no scaling.
    ///
    /// Nothing is re-laid-out: taffy still measures the element at its natural
    /// size and siblings stay put, so a button that scales to 0.95 on press
    /// doesn't shove the row around. Everything drawn inside scales with it —
    /// rect geometry, corner radii, border and shadow widths, font sizes,
    /// polyline points — and so does the hit region, which keeps the pointer
    /// and the pixels in agreement.
    ///
    /// This is what `.hover(|s| s.scale(1.1))` / `.active(|s| s.scale(0.95))`
    /// fold into.
    pub scale: f32,

    // Text (inherited)
    pub color: Color,
    pub font_size: f32,
    pub bold: bool,
    /// 斜体。 フォントが斜体の face を持たない場合は cosmic-text が傾けて代用する。
    pub italic: bool,
    pub monospace: bool,
    /// 折り返した行の揃え方。 折り返しが起きない 1 行のテキストでは効かない
    /// (要素の箱が中身ぴったりなので、 揃える余白が無い)。
    pub text_align: TextAlign,
    /// Specific font family for this element's text, overriding the generic
    /// (and any app-level preferred) family at shaping time. `None` (default)
    /// keeps the normal resolution: monospace/sans-serif generics, optionally
    /// redirected by the renderer's preferred families. Set it to render text
    /// in a *named* face regardless of app defaults — e.g. a font picker
    /// previewing each row in its own font (Word-style).
    pub font_family: Option<String>,
    /// Explicit font weight (100–900) overriding `bold`. `None` (default)
    /// derives from `bold` (700 when bold, else 400). Set via `.font_weight(n)`.
    pub font_weight: Option<u16>,
    /// Extra inter-glyph tracking in logical px (`.letter_spacing(px)`).
    /// `0.0` (default) = the font's natural advances. Negative tightens.
    pub letter_spacing: f32,
    /// Line height as a multiple of `font_size` (`.line_height(mult)`).
    /// `None` (default) = 1.4, the framework-wide default.
    pub line_height: Option<f32>,
    /// Maximum visible line count for wrapped text. `None` (default)
    /// means unlimited — wrap to as many lines as the content needs
    /// (Excel-cell style). `Some(n)` clips to `n` lines and appends
    /// `…` at the end of line `n` if more content follows. Combined
    /// with the container's `max_width` (or its computed width when
    /// inside a sized parent), this gives CSS `line-clamp` semantics
    /// for declarative apps.
    pub max_lines: Option<u32>,
    /// Framework-drawn scrollbar thumb color for `Overflow::Scroll`
    /// containers (`.scrollbar(color)`). `None` (default) draws nothing.
    /// When set, a thin rounded thumb is drawn at the container's right
    /// edge whenever the content overflows vertically — position/size from
    /// the same animated scroll offset the content renders with. Indicator
    /// only: it adds no hit region, so click/wheel routing is untouched.
    pub scrollbar_thumb: Option<Color>,
    /// Per-range background highlights for this element's text. Empty
    /// (default) draws nothing. Only text/button draws read it. Set via
    /// `.highlight(HighlightSpec { .. })`, which appends — see there for why
    /// this is a list rather than a single spec.
    pub highlight: Vec<HighlightSpec>,
    /// Clickable/hoverable byte ranges in a text element (in-body links).
    /// `.link_ranges(..)`.
    pub link_ranges: Option<Vec<LinkRange>>,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            position: Position::Relative,
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Stretch,
            justify_content: JustifyContent::Start,
            align_self: AlignSelf::Auto,
            justify_self: AlignSelf::Auto,
            align_content: None,
            justify_items: None,
            aspect_ratio: None,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_flow: GridAutoFlow::Row,
            grid_column: (GridPlacement::Auto, GridPlacement::Auto),
            grid_row: (GridPlacement::Auto, GridPlacement::Auto),
            z_index: 0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            gap: 0.0,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: EdgeDimensions::default(),
            margin: EdgeDimensions::all(Dimension::Px(0.0)),
            overflow: Overflow::Visible,
            inset_top: Dimension::Auto,
            inset_right: Dimension::Auto,
            inset_bottom: Dimension::Auto,
            inset_left: Dimension::Auto,
            scroll_x: 0.0,
            scroll_y: 0.0,
            scroll_owner: ScrollOwner::Runtime,
            background: Color::TRANSPARENT,
            gradient_end: Color::TRANSPARENT,
            gradient_angle: 0.0,
            corner_radius: Corners::all(0.0),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            shadow: None,
            opacity: 1.0,
            object_fit: ObjectFit::default(),
            rotation: 0.0,
            translate_x: 0.0,
            translate_y: 0.0,
            scale: 1.0,
            color: Color::WHITE,
            font_size: 14.0,
            bold: false,
            italic: false,
            monospace: false,
            text_align: TextAlign::Start,
            font_family: None,
            font_weight: None,
            letter_spacing: 0.0,
            line_height: None,
            max_lines: None,
            scrollbar_thumb: None,
            highlight: Vec::new(),
            link_ranges: None,
        }
    }
}

/// Extended typographic controls layered on top of `font_size` / `bold` /
/// `monospace`. Bundled into one value so the shaping path (measure + render)
/// threads a single param instead of three loose ones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    /// Explicit weight (100–900). `None` derives from `bold`
    /// (700 when bold, else 400).
    pub weight: Option<u16>,
    /// Extra tracking between glyphs, in logical px. `0.0` = natural advances.
    pub letter_spacing: f32,
    /// Line height as a multiple of the font size. `None` = 1.4.
    pub line_height: Option<f32>,
    /// 斜体。 face に斜体が無ければ合成斜体になる。
    pub italic: bool,
    /// 折り返した行の揃え方。 **幅が決まっている要素でのみ効く** — 揃える
    /// 相手の余白が無ければ何も起きない。
    pub align: TextAlign,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            weight: None,
            letter_spacing: 0.0,
            line_height: None,
            italic: false,
            align: TextAlign::Start,
        }
    }
}

impl Typography {
    /// Numeric weight to shape with, honoring `bold` when `weight` is unset.
    pub fn resolved_weight(&self, bold: bool) -> u16 {
        self.weight.unwrap_or(if bold { 700 } else { 400 })
    }

    /// Absolute line height in logical px for a given font size.
    pub fn line_height_px(&self, font_size: f32) -> f32 {
        font_size * self.line_height.unwrap_or(1.4)
    }
}

impl ElementStyle {
    /// Bundle this style's extended typographic controls for the shaper.
    pub fn typography(&self) -> Typography {
        Typography {
            weight: self.font_weight,
            letter_spacing: self.letter_spacing,
            line_height: self.line_height,
            italic: self.italic,
            align: self.text_align,
        }
    }
}

// ---------------------------------------------------------------------------
// Event callbacks
// ---------------------------------------------------------------------------

/// Boxed event callback. Use `Box<dyn FnMut()>` for click handlers etc.
pub type EventHandler = Box<dyn FnMut() + 'static>;

// ---------------------------------------------------------------------------
// Transition & interactive state styles
// ---------------------------------------------------------------------------

/// Which property to animate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransitionProperty {
    Background,
    BorderColor,
    Shadow,
    Opacity,
    Transform,
    All,
}

/// Animation type for transitions.
#[derive(Clone, Copy, Debug)]
pub enum TransitionKind {
    /// Duration-based easing.
    Easing { duration: f32, function: EasingFn },
    /// Physics-based spring.
    Spring { stiffness: f32, damping: f32 },
}

impl Default for TransitionKind {
    fn default() -> Self {
        Self::Spring {
            stiffness: 200.0,
            damping: 20.0,
        }
    }
}

/// CSS-style easing (subset, inlined to avoid cross-crate dep).
#[derive(Clone, Copy, Debug)]
pub enum EasingFn {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

/// A single transition declaration.
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    pub property: TransitionProperty,
    pub kind: TransitionKind,
}

/// Style overrides for hover / active / focus states.
/// Only `Some` fields override the base style.
#[derive(Clone, Debug, Default)]
pub struct StateStyle {
    pub background: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub shadow: Option<Option<BoxShadow>>,
    pub opacity: Option<f32>,
    pub corner_radius: Option<Corners<f32>>,
    /// Scale transform (1.0 = normal).
    pub scale: Option<f32>,
    /// X offset (for slide animations).
    pub translate_x: Option<f32>,
    /// Y offset (for slide animations).
    pub translate_y: Option<f32>,
    /// Gap override.
    pub gap: Option<f32>,
    /// Width override.
    pub width: Option<Dimension>,
    /// Height override.
    pub height: Option<Dimension>,
    /// Padding override (all sides).
    pub padding: Option<EdgeDimensions>,
    /// Text color override.
    pub color: Option<Color>,
    /// Font size override.
    pub font_size: Option<f32>,
}

impl StateStyle {
    /// Merge this state style onto a base ElementStyle, returning a new resolved style.
    ///
    /// フィールドの取りこぼしを防ぐため、畳み込み自体は [`fold_state_style`] に
    /// 一本化してある（かつて 3 箇所に同じ列挙があり、ここだけ `scale` /
    /// `translate_*` を落としていた）。
    pub fn apply_to(&self, base: &ElementStyle) -> ElementStyle {
        let mut s = base.clone();
        fold_state_style(&mut s, self, false);
        s
    }
}

// ---------------------------------------------------------------------------
// Element — the core tree node
// ---------------------------------------------------------------------------

/// A declarative UI element. Constructed via `div()`, `text()`, or `button()`.
pub struct Element {
    pub kind: ElementKind,
    pub style: ElementStyle,
    pub children: Vec<Element>,
    /// Unique identifier for event dispatch (click/hover).
    pub id: Option<String>,
    pub on_click: Option<EventHandler>,
    pub on_hover: Option<EventHandler>,
    /// Whether this element can receive focus.
    pub focusable: bool,
    /// 支援技術に伝える役割。 `None` は「意味を持たない入れ物」(= [`Role::Group`])。
    /// [`Element::role`] で設定する。
    pub role: Option<Role>,
    /// 支援技術が読み上げる名前 (ARIA の `aria-label` 相当)。
    ///
    /// 中の文字がそのまま名前になる場合 (ボタンのラベル等) は不要。 アイコンだけの
    /// ボタンや画像のように、 **見た目からは名前が取れない**ものに付ける。
    /// [`Element::label`] で設定する。
    pub label: Option<String>,
    /// 見出しの階層 (1 が最上位)。 [`Role::Heading`] のときだけ意味がある。
    pub heading_level: Option<u8>,
    /// Style overrides when hovered.
    pub hover_style: Option<StateStyle>,
    /// Style overrides when pressed/active.
    pub active_style: Option<StateStyle>,
    /// Transition declarations.
    pub transitions: Vec<Transition>,
    /// When true, this element (and all its children) renders on the overlay
    /// layer — drawn after all base-layer content so it appears on top.
    pub overlay: bool,
    /// Tooltip text shown on hover after a short delay.
    pub tooltip: Option<String>,
    /// Drag payload data. When set, this element can be dragged.
    pub drag_data: Option<String>,
    /// Whether this element is a drop zone (accepts dragged items).
    pub drop_zone: bool,
    /// Whether this element should animate in/out when added/removed from the tree.
    /// Requires `.id()` to be set for tracking.
    pub animate_presence: bool,
    /// Mouse cursor to display while hovering this element. `None` means
    /// "no opinion" — the runtime falls back to whatever the parent
    /// requests, or to the platform default if no ancestor opted in.
    /// Use `.cursor(Cursor::Pointer)` for clickable widgets that should
    /// show the hand cursor (buttons, links, tiles); leave it `None`
    /// for menu rows / panel bodies that should keep the default arrow
    /// per macOS HIG.
    pub cursor: Option<Cursor>,
    /// Opt this element (and its whole subtree) out of text selection —
    /// the CSS `user-select: none` equivalent. Text under a `no_select`
    /// ancestor never becomes a selection anchor/head, is never painted
    /// with the selection background, and is skipped by clipboard extract.
    ///
    /// Use it on chrome — toolbars, sidebars, table headers, status bars —
    /// so that dragging inside a panel doesn't smear a blue highlight over
    /// every label. Prose stays selectable because you simply don't set it
    /// there. Inherits down, so one call on the panel root covers it.
    ///
    /// Button labels are non-selectable regardless of this flag: a control
    /// label is not content.
    pub no_select: bool,
}

/// Mouse-cursor preference for an [`Element`]. Mirrors the platform
/// values most apps actually need; runtime backends translate to their
/// native cursor type (winit `CursorIcon`, NSCursor, etc.). Add new
/// variants here as concrete needs arise rather than mirroring the full
/// matrix of every platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// Default arrow cursor.
    Default,
    /// Hand cursor for clickable elements (buttons, links).
    Pointer,
    /// I-beam cursor for text input regions.
    Text,
    /// Crosshair cursor for selection / picking.
    Crosshair,
    /// "Not allowed" cursor for disabled / forbidden actions.
    NotAllowed,
    /// Horizontal resize (`ew-resize`) cursor — drag-to-adjust affordance
    /// for numeric inputs and split panes.
    ResizeEw,
    /// Vertical resize (`ns-resize`) cursor — 上下に分割したペインの仕切り。
    ResizeNs,
}

#[derive(Clone, Debug)]
pub enum ElementKind {
    Div,
    Text { content: String },
    Button { label: String, accent: Option<Color> },
    Image { key: String, data: ImageData },
    /// Arc / ring segment — SDF-rasterized donut sector with separate
    /// active "fill" and inactive "track" colors. The element's layout
    /// rect defines the arc's bounding box; the arc fits inside the
    /// largest centered square. Use for tachometer gauges, activity
    /// rings, progress wheels.
    Arc(ArcKind),
    /// Polyline — an open sequence of connected segments, SDF-rasterized
    /// as capsules (see `line.wgsl`). Points are logical px relative to
    /// the element's layout box origin. Use for charts, sparklines,
    /// connectors.
    Polyline(PolylineKind),
}

/// Layout-relative polyline parameters. Wrapped inside
/// [`ElementKind::Polyline`]. Points are in logical px relative to the
/// element's layout-box origin, so a chart widget can size a box and
/// plot into its local `0..w × 0..h` space.
#[derive(Clone, Debug)]
pub struct PolylineKind {
    /// Vertices in logical px, relative to the element's layout box origin.
    pub points: Vec<(f32, f32)>,
    /// Stroke width (logical px).
    pub width: f32,
    /// Stroke color.
    pub color: Color,
}

/// Layout-independent arc parameters. Wrapped inside [`ElementKind::Arc`].
#[derive(Clone, Copy, Debug)]
pub struct ArcKind {
    /// Active fill fraction in `[0, 1]`. Pixels in the angular range
    /// `[start_angle, start_angle + value * sweep_angle]` use
    /// `fill_color`; the remainder uses `track_color`.
    pub value: f32,
    /// First angle of the full arc (radians, screen-space convention:
    /// y grows down, so `0.75π` is bottom-left, `1.5π` is straight up).
    pub start_angle: f32,
    /// Total angular sweep of the arc (radians).
    pub sweep_angle: f32,
    /// Donut band width (logical px). Inner radius is computed as
    /// `outer_radius - thickness` where `outer_radius` is half the
    /// element's smaller layout dimension.
    pub thickness: f32,
    /// Color of the active fill arc.
    pub fill_color: Color,
    /// Color of the inactive track arc.
    pub track_color: Color,
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Element")
            .field("kind", &self.kind)
            .field("style", &self.style)
            .field("children", &self.children)
            .field("id", &self.id)
            .field("on_click", &self.on_click.as_ref().map(|_| ".."))
            .field("on_hover", &self.on_hover.as_ref().map(|_| ".."))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// State style folding (hover / active)
// ---------------------------------------------------------------------------

/// Fold the hover and press state styles of a whole tree into its base styles.
///
/// Runs on the element tree *before* layout, so state overrides that change
/// geometry (`width` / `height` / `padding` / `gap`) work like any other style —
/// taffy simply measures the folded values.
///
/// Precedence is press over hover over base, matching
/// `sabitori_scene::NodeStyle::effective_style`. Both are folded when both
/// apply, so `.active()` only has to name what actually differs while pressed.
///
/// `animated` elements (those with `transitions`) are handled by
/// `StyleAnimator` for the fields it interpolates; see [`fold_state_style`].
///
/// Both runtimes (`DeclarativeApp` and `SceneApp`) call this, so a widget
/// resolves its states identically no matter which one is driving it.
pub fn apply_state_styles(
    element: &mut Element,
    hovered_id: &Option<String>,
    pressed_id: &Option<String>,
) {
    let is_hovered = element
        .id
        .as_deref()
        .is_some_and(|id| hovered_id.as_deref() == Some(id));
    let is_pressed = element
        .id
        .as_deref()
        .is_some_and(|id| pressed_id.as_deref() == Some(id));
    if is_hovered || is_pressed {
        let animated = !element.transitions.is_empty();
        // style と hover_style/active_style を同時に触るので、フィールド分割で
        // 借用を割る。
        let Element { style, hover_style, active_style, .. } = element;
        if is_hovered {
            if let Some(h) = hover_style.as_ref() {
                fold_state_style(style, h, animated);
            }
        }
        if is_pressed {
            if let Some(a) = active_style.as_ref() {
                fold_state_style(style, a, animated);
            }
        }
    }
    for child in &mut element.children {
        apply_state_styles(child, hovered_id, pressed_id);
    }
}

/// Fold one [`StateStyle`] into an [`ElementStyle`].
///
/// `animated` = この要素は `transitions` を持つので `StyleAnimator` がバネで
/// 補間している。その場合、animator が持っているフィールドをここで即値にすると
/// 補間を潰してしまうので飛ばす。
///
/// animator が扱わないフィールド（scale / translate / 角丸 / 影 / レイアウト系）は
/// `transitions` の有無に関わらず即時に反映する — さもないと
/// `.spring_transition()` を足した瞬間に `.active(|s| s.scale(0.95))` が黙って
/// 効かなくなる、という一番たちの悪い形になる。
pub fn fold_state_style(style: &mut ElementStyle, s: &StateStyle, animated: bool) {
    // --- StyleAnimator が扱わない = 常に即時 ---
    if let Some(v) = s.scale { style.scale = v; }
    if let Some(v) = s.translate_x { style.translate_x = v; }
    if let Some(v) = s.translate_y { style.translate_y = v; }
    if let Some(v) = s.corner_radius { style.corner_radius = v; }
    if let Some(v) = s.shadow { style.shadow = v; }
    if let Some(v) = s.gap { style.gap = v; }
    if let Some(v) = s.width { style.width = v; }
    if let Some(v) = s.height { style.height = v; }
    if let Some(v) = s.padding { style.padding = v; }
    // --- ここから下は StyleAnimator の担当 ---
    if animated {
        return;
    }
    if let Some(v) = s.background { style.background = v; }
    if let Some(v) = s.border_color { style.border_color = v; }
    if let Some(v) = s.border_width { style.border_width = v; }
    if let Some(v) = s.opacity { style.opacity = v; }
    if let Some(v) = s.color { style.color = v; }
    if let Some(v) = s.font_size { style.font_size = v; }
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

/// Create a div element (generic container).
pub fn div() -> Element {
    Element {
        kind: ElementKind::Div,
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        role: None,
        label: None,
        heading_level: None,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// grid の入れ物を作る。 `div().grid()` と同じ。
///
/// ```ignore
/// grid()
///     .grid_cols([Track::px(240.0), Track::fr(1.0)])
///     .gap(12.0)
///     .children([sidebar, main])
/// ```
pub fn grid() -> Element {
    div().grid()
}

/// Create a text element.
pub fn text(content: impl Into<String>) -> Element {
    Element {
        kind: ElementKind::Text { content: content.into() },
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        role: None,
        label: None,
        heading_level: None,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create a polyline element — an open sequence of connected line
/// segments, SDF-rasterized with round joints. Points are logical px
/// relative to the element's layout box (set width/height so it has a
/// box, or place points in absolute-ish coords within a full-size box).
/// Use for charts, sparklines, connectors. Set vertices with `.points`
/// and stroke with `.stroke_width` / `.stroke_color`.
pub fn polyline() -> Element {
    Element {
        kind: ElementKind::Polyline(PolylineKind {
            points: Vec::new(),
            width: 1.5,
            color: Color::TRANSPARENT,
        }),
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        role: None,
        label: None,
        heading_level: None,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create an arc / ring element.
///
/// Defaults: 270° sweep starting at the bottom-left (tachometer
/// orientation), `value = 0.0`, 8-px thickness, transparent fill +
/// track. Tweak via the builder methods (`.arc_value`,
/// `.arc_thickness`, `.arc_colors`, `.arc_sweep`).
///
/// The element has a default size of 0×0; set width/height (or use a
/// flex parent) so it gets a real bounding box. The renderer fits
/// the arc inside the largest centered square of that bounding box.
pub fn arc() -> Element {
    Element {
        kind: ElementKind::Arc(ArcKind {
            value: 0.0,
            start_angle: std::f32::consts::PI * 0.75,
            sweep_angle: std::f32::consts::PI * 1.5,
            thickness: 8.0,
            fill_color: Color::TRANSPARENT,
            track_color: Color::TRANSPARENT,
        }),
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        role: None,
        label: None,
        heading_level: None,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create an image element.
pub fn image(key: impl Into<String>, data: ImageData) -> Element {
    Element {
        kind: ElementKind::Image { key: key.into(), data },
        style: ElementStyle::default(),
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        role: None,
        label: None,
        heading_level: None,
        hover_style: None,
        active_style: None,
        transitions: Vec::new(),
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

/// Create a button element with default interactive styles.
pub fn button(label: impl Into<String>) -> Element {
    let mut style = ElementStyle::default();
    style.flex_direction = FlexDirection::Row;
    style.align_items = AlignItems::Center;
    style.justify_content = JustifyContent::Center;
    style.padding = EdgeDimensions::axes(Dimension::Px(8.0), Dimension::Px(16.0));
    style.corner_radius = Corners::all(6.0);

    Element {
        kind: ElementKind::Button { label: label.into(), accent: None },
        style,
        children: Vec::new(),
        id: None,
        on_click: None,
        on_hover: None,
        focusable: false,
        // ボタンは既定で役割を名乗る。 支援技術から「押せるもの」として見える
        // かどうかを、 呼び出し側が毎回書かないで済むように (issue #21)。
        // 名前は中のラベルから取れるので `label` は None のまま。
        role: Some(Role::Button),
        label: None,
        heading_level: None,
        // A button ships with the affordance built in: it lifts a little under
        // the pointer and sinks under the press. Colors are deliberately not
        // touched — the right hover tint depends on the app's palette, and an
        // `.accent()` button would fight a hardcoded one. Scale is palette-free,
        // so it reads correctly on any theme.
        //
        // Callers who want something else just override with `.hover()` /
        // `.active()`; those replace these outright.
        hover_style: Some(StateStyle { scale: Some(1.02), ..StateStyle::default() }),
        active_style: Some(StateStyle { scale: Some(0.96), ..StateStyle::default() }),
        transitions: vec![Transition {
            property: TransitionProperty::All,
            kind: TransitionKind::default(),
        }],
        overlay: false,
        tooltip: None,
        drag_data: None,
        drop_zone: false,
        animate_presence: false,
        cursor: None,
        no_select: false,
    }
}

// ---------------------------------------------------------------------------
// Builder methods (chainable)
// ---------------------------------------------------------------------------

impl Element {
    // -- Sizing --

    /// Set width.
    pub fn w(mut self, d: Dimension) -> Self {
        self.style.width = d;
        self
    }

    /// Set height.
    pub fn h(mut self, d: Dimension) -> Self {
        self.style.height = d;
        self
    }

    /// `width: 100%`
    pub fn w_full(mut self) -> Self {
        self.style.width = Dimension::Percent(100.0);
        self
    }

    /// `height: 100%`
    pub fn h_full(mut self) -> Self {
        self.style.height = Dimension::Percent(100.0);
        self
    }

    /// Set min-width.
    pub fn min_w(mut self, d: Dimension) -> Self {
        self.style.min_width = d;
        self
    }

    /// Set min-height.
    pub fn min_h(mut self, d: Dimension) -> Self {
        self.style.min_height = d;
        self
    }

    /// Set max-width.
    pub fn max_w(mut self, d: Dimension) -> Self {
        self.style.max_width = d;
        self
    }

    /// Set max-height.
    pub fn max_h(mut self, d: Dimension) -> Self {
        self.style.max_height = d;
        self
    }

    /// Set both width and height.
    pub fn size(mut self, w: Dimension, h: Dimension) -> Self {
        self.style.width = w;
        self.style.height = h;
        self
    }

    // -- Background / Visual --

    /// Set background color.
    pub fn bg(mut self, color: Color) -> Self {
        self.style.background = color;
        self
    }

    /// Set a linear gradient background.
    /// `angle` is in radians (0 = left→right, PI/2 = top→bottom).
    /// ラジアン回転を加える。正 = 画面上時計回り。
    ///
    /// 矩形は中心まわり (細長い rect を回転させて線として使う)、テキストは原点
    /// (左上) まわり — ピボットが違う。詳細は [`ElementStyle::rotation`]。
    pub fn rotation(mut self, radians: f32) -> Self {
        self.style.rotation = radians;
        self
    }

    pub fn gradient(mut self, from: Color, to: Color, angle: f32) -> Self {
        self.style.background = from;
        self.style.gradient_end = to;
        self.style.gradient_angle = angle;
        self
    }

    /// Set corner radius (all corners).
    pub fn rounded(mut self, d: Dimension) -> Self {
        let r = match d {
            Dimension::Px(v) => v,
            Dimension::Percent(v) => v, // percentage of min(width, height) resolved later
            Dimension::Auto => 0.0,
        };
        self.style.corner_radius = Corners::all(r);
        self
    }

    /// Set corner radius with a raw f32 (pixels).
    pub fn rounded_px(mut self, r: f32) -> Self {
        self.style.corner_radius = Corners::all(r);
        self
    }

    /// Set per-corner radius.
    pub fn corner_radius(mut self, corners: Corners<f32>) -> Self {
        self.style.corner_radius = corners;
        self
    }

    /// Set border.
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.style.border_width = width;
        self.style.border_color = color;
        self
    }

    /// Set border width only.
    pub fn border_width(mut self, width: f32) -> Self {
        self.style.border_width = width;
        self
    }

    /// Set border color only.
    pub fn border_color(mut self, color: Color) -> Self {
        self.style.border_color = color;
        self
    }

    /// Set box shadow.
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.style.shadow = Some(shadow);
        self
    }

    /// Quick shadow: `shadow_sm()` for a subtle elevation.
    pub fn shadow_sm(mut self, color: Color) -> Self {
        self.style.shadow = Some(BoxShadow {
            color,
            offset: Point::new(0.0, 2.0),
            blur: 4.0,
            spread: 0.0,
        });
        self
    }

    /// Quick shadow: `shadow_md()` for medium elevation.
    pub fn shadow_md(mut self, color: Color) -> Self {
        self.style.shadow = Some(BoxShadow {
            color,
            offset: Point::new(0.0, 4.0),
            blur: 8.0,
            spread: 0.0,
        });
        self
    }

    /// Glow effect (centered shadow with no offset).
    pub fn glow(mut self, color: Color, radius: f32) -> Self {
        self.style.shadow = Some(BoxShadow {
            color,
            offset: Point::ZERO,
            blur: radius,
            spread: 0.0,
        });
        self
    }

    /// Subtle glow (30% alpha, 8px blur).
    pub fn glow_sm(mut self, color: Color) -> Self {
        self.glow(color.with_alpha(0.3), 8.0)
    }

    /// Set opacity (0.0 = transparent, 1.0 = opaque).
    pub fn opacity(mut self, v: f32) -> Self {
        self.style.opacity = v;
        self
    }

    /// Set object-fit mode for Image elements.
    pub fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.style.object_fit = fit;
        self
    }

    // -- Padding --

    /// Set padding (all sides).
    pub fn p(mut self, d: Dimension) -> Self {
        self.style.padding = EdgeDimensions::all(d);
        self
    }

    /// Set padding with a raw f32 (pixels).
    pub fn p_px(mut self, v: f32) -> Self {
        self.style.padding = EdgeDimensions::all(Dimension::Px(v));
        self
    }

    /// Set horizontal padding.
    pub fn px_pad(mut self, d: Dimension) -> Self {
        self.style.padding.left = d;
        self.style.padding.right = d;
        self
    }

    /// Set vertical padding.
    pub fn py(mut self, d: Dimension) -> Self {
        self.style.padding.top = d;
        self.style.padding.bottom = d;
        self
    }

    /// Set padding-top.
    pub fn pt(mut self, d: Dimension) -> Self {
        self.style.padding.top = d;
        self
    }

    /// Set padding-right.
    pub fn pr(mut self, d: Dimension) -> Self {
        self.style.padding.right = d;
        self
    }

    /// Set padding-bottom.
    pub fn pb(mut self, d: Dimension) -> Self {
        self.style.padding.bottom = d;
        self
    }

    /// Set padding-left.
    pub fn pl(mut self, d: Dimension) -> Self {
        self.style.padding.left = d;
        self
    }

    // -- Margin --

    /// Set margin (all sides).
    pub fn m(mut self, d: Dimension) -> Self {
        self.style.margin = EdgeDimensions::all(d);
        self
    }

    /// Set margin with a raw f32 (pixels).
    pub fn m_px(mut self, v: f32) -> Self {
        self.style.margin = EdgeDimensions::all(Dimension::Px(v));
        self
    }

    /// Set horizontal margin.
    pub fn mx(mut self, d: Dimension) -> Self {
        self.style.margin.left = d;
        self.style.margin.right = d;
        self
    }

    /// Set vertical margin.
    pub fn my(mut self, d: Dimension) -> Self {
        self.style.margin.top = d;
        self.style.margin.bottom = d;
        self
    }

    /// Set margin-top.
    pub fn mt(mut self, d: Dimension) -> Self {
        self.style.margin.top = d;
        self
    }

    /// Set margin-right.
    pub fn mr(mut self, d: Dimension) -> Self {
        self.style.margin.right = d;
        self
    }

    /// Set margin-bottom.
    pub fn mb(mut self, d: Dimension) -> Self {
        self.style.margin.bottom = d;
        self
    }

    /// Set margin-left.
    pub fn ml(mut self, d: Dimension) -> Self {
        self.style.margin.left = d;
        self
    }

    // -- Flex layout --

    /// Set flex direction to column.
    pub fn flex_col(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Column;
        self
    }

    /// Set flex direction to row.
    pub fn flex_row(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Row;
        self
    }

    /// Set flex direction.
    pub fn flex_direction(mut self, dir: FlexDirection) -> Self {
        self.style.flex_direction = dir;
        self
    }

    /// Set flex wrap.
    pub fn flex_wrap(mut self, wrap: FlexWrap) -> Self {
        self.style.flex_wrap = wrap;
        self
    }

    /// Enable wrapping.
    pub fn wrap(mut self) -> Self {
        self.style.flex_wrap = FlexWrap::Wrap;
        self
    }

    /// Set flex grow.
    pub fn grow(mut self, v: f32) -> Self {
        self.style.flex_grow = v;
        self
    }

    /// Shortcut: `flex: 1 1 0%` (Tailwind `flex-1`).
    ///
    /// Sets `flex_grow: 1` AND `flex_basis: 0` so the item's share of the
    /// parent's main axis is computed from *free space*, not from its own
    /// content size. The basis-0 part is what makes
    /// `flex_1().scroll(id)` work without an explicit height: with
    /// the old `flex_basis: auto` behavior the scroll container's flex base
    /// size was its full content height, so it overflowed its slot (and
    /// squeezed its siblings) instead of scrolling. Use `.grow(1.0)` if you
    /// explicitly want grow-from-content behavior.
    pub fn flex_1(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self.style.flex_basis = Dimension::Px(0.0);
        self
    }

    /// Set the flex base size (CSS `flex-basis`). Default `Auto`.
    pub fn basis(mut self, d: Dimension) -> Self {
        self.style.flex_basis = d;
        self
    }

    /// Set flex shrink.
    pub fn shrink(mut self, v: f32) -> Self {
        self.style.flex_shrink = v;
        self
    }

    /// Set gap between children (pixels).
    pub fn gap(mut self, v: f32) -> Self {
        self.style.gap = v;
        self
    }

    /// Set align-items.
    pub fn align_items(mut self, a: AlignItems) -> Self {
        self.style.align_items = a;
        self
    }

    /// Shortcut: `align-items: center`.
    pub fn items_center(mut self) -> Self {
        self.style.align_items = AlignItems::Center;
        self
    }

    /// Shortcut: `align-items: start`.
    pub fn items_start(mut self) -> Self {
        self.style.align_items = AlignItems::Start;
        self
    }

    /// Shortcut: `align-items: end`.
    pub fn items_end(mut self) -> Self {
        self.style.align_items = AlignItems::End;
        self
    }

    /// Set justify-content.
    pub fn justify_content(mut self, j: JustifyContent) -> Self {
        self.style.justify_content = j;
        self
    }

    /// Shortcut: `justify-content: center`.
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = JustifyContent::Center;
        self
    }

    /// Shortcut: `justify-content: space-between`.
    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = JustifyContent::SpaceBetween;
        self
    }

    /// Shortcut: `justify-content: end`.
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = JustifyContent::End;
        self
    }

    // -- align-self / align-content / aspect-ratio --

    /// この子だけ親の `align_items` から外れる (CSS `align-self`)。
    pub fn align_self(mut self, a: AlignSelf) -> Self {
        self.style.align_self = a;
        self
    }

    /// Shortcut: `align-self: start`.
    pub fn self_start(self) -> Self {
        self.align_self(AlignSelf::Start)
    }

    /// Shortcut: `align-self: center`.
    pub fn self_center(self) -> Self {
        self.align_self(AlignSelf::Center)
    }

    /// Shortcut: `align-self: end`.
    pub fn self_end(self) -> Self {
        self.align_self(AlignSelf::End)
    }

    /// Shortcut: `align-self: stretch`. 親が `items_center()` でも、 これを
    /// 付けた子だけは交差軸いっぱいに伸びる。
    pub fn self_stretch(self) -> Self {
        self.align_self(AlignSelf::Stretch)
    }

    /// grid のセル内での主軸方向の寄せ。 flex では無視される。
    pub fn justify_self(mut self, a: AlignSelf) -> Self {
        self.style.justify_self = a;
        self
    }

    /// 折り返した**行**そのものの配り方 (CSS `align-content`)。
    /// [`Element::wrap`] を付けた入れ物か grid でだけ効く。
    pub fn align_content(mut self, a: AlignContent) -> Self {
        self.style.align_content = Some(a);
        self
    }

    /// grid のセル内での主軸方向の寄せの既定値。 子の `justify_self` が優先。
    pub fn justify_items(mut self, a: AlignItems) -> Self {
        self.style.justify_items = Some(a);
        self
    }

    /// 幅 ÷ 高さの比を固定する (CSS `aspect-ratio`)。
    ///
    /// 片方の辺だけ決めれば、 もう片方がこの比で決まる。 サムネイルを
    /// 16:9 に揃える、 アイコンを正方形に保つ、 といった用途。
    ///
    /// ```ignore
    /// div().w_full().aspect(16.0 / 9.0)   // 幅なりの 16:9
    /// div().h(Px(40.0)).aspect(1.0)       // 40x40 の正方形
    /// ```
    pub fn aspect(mut self, ratio: f32) -> Self {
        self.style.aspect_ratio = Some(ratio);
        self
    }

    // -- Grid --

    /// この要素を grid にする。 列は [`Element::grid_cols`] で引く。
    pub fn grid(mut self) -> Self {
        self.style.display = Display::Grid;
        self
    }

    /// grid の列を引く。 **付けると `display: grid` になる**ので
    /// [`Element::grid`] を別途呼ぶ必要は無い。
    ///
    /// ```ignore
    /// div().grid_cols([Track::px(240.0), Track::fr(1.0)]).gap(12.0)
    /// ```
    pub fn grid_cols(mut self, tracks: impl IntoIterator<Item = Track>) -> Self {
        self.style.display = Display::Grid;
        self.style.grid_template_columns = tracks.into_iter().collect();
        self
    }

    /// grid の行を引く。 [`Element::grid_cols`] と同じく `display: grid` になる。
    pub fn grid_rows(mut self, tracks: impl IntoIterator<Item = Track>) -> Self {
        self.style.display = Display::Grid;
        self.style.grid_template_rows = tracks.into_iter().collect();
        self
    }

    /// 自動配置の進み方。
    pub fn grid_flow(mut self, flow: GridAutoFlow) -> Self {
        self.style.grid_auto_flow = flow;
        self
    }

    /// この要素が占める列を (開始, 終了) で指定する。
    pub fn col(mut self, start: GridPlacement, end: GridPlacement) -> Self {
        self.style.grid_column = (start, end);
        self
    }

    /// この要素が占める行を (開始, 終了) で指定する。
    pub fn row(mut self, start: GridPlacement, end: GridPlacement) -> Self {
        self.style.grid_row = (start, end);
        self
    }

    /// 列を `n` 本ぶんまたぐ。 開始位置は自動。
    pub fn col_span(mut self, n: u16) -> Self {
        self.style.grid_column = (GridPlacement::Span(n), GridPlacement::Auto);
        self
    }

    /// 行を `n` 本ぶんまたぐ。 開始位置は自動。
    pub fn row_span(mut self, n: u16) -> Self {
        self.style.grid_row = (GridPlacement::Span(n), GridPlacement::Auto);
        self
    }

    // -- Stacking --

    /// 兄弟の中での重なり順。 大きいほど手前に描かれ、 クリックも先に取る。
    /// 既定は `0` で、 同値なら書いた順 (後ろが手前)。
    ///
    /// # 親は飛び越えない
    ///
    /// **効くのは同じ親を持つ兄弟の中だけ**。 `z(999)` を書いても、 親より
    /// 手前の要素の上には出ない。 CSS の重なり文脈と同じ制約で、 sabitori では
    /// 親の `overflow_hidden` によるクリップが子に効いている以上、 これを
    /// 破ると「クリップの外に描かれる要素」が生まれてしまう。
    ///
    /// 木を飛び越えて最前面に出したいなら [`Element::overlay`] — ポップアップや
    /// コンテキストメニューはそちら。
    ///
    /// ```ignore
    /// // 重なった丸を、書いた順と逆に見せる
    /// div().children([
    ///     avatar(a).z(3),
    ///     avatar(b).z(2),
    ///     avatar(c).z(1),
    /// ])
    /// ```
    pub fn z(mut self, z: i32) -> Self {
        self.style.z_index = z;
        self
    }

    /// Set overflow behavior.
    ///
    /// # スクロールさせたいなら [`Element::scroll`] を使うこと
    ///
    /// `.overflow(Overflow::Scroll)` は**生の逃げ道**で、 スクロール位置を持つ主体が
    /// 決まらない。 id が無ければランタイムは状態を引けないので管理対象にならず、
    /// ホイールも慣性も効かないまま「クリップだけされる箱」になる。
    ///
    /// ```ignore
    /// div().scroll("rows").flex_1().flex_col().children(rows)      // ランタイムが位置を持つ
    /// div().scroll_manual(0.0, self.y).flex_col().children(rows)   // アプリが位置を持つ
    /// ```
    ///
    /// 0.4.0 より前の `.overflow_scroll()` は上の 2 つに分かれた。 どちらでもない
    /// `.overflow(Overflow::Scroll)` は、 スクロールの主体を自分で書く場合だけ使う。
    pub fn overflow(mut self, o: Overflow) -> Self {
        self.style.overflow = o;
        self
    }

    /// Set overflow to hidden.
    pub fn overflow_hidden(mut self) -> Self {
        self.style.overflow = Overflow::Hidden;
        self
    }

    /// スクロールコンテナにする。 位置はランタイムが `id` をキーに保持し、
    /// ホイール・慣性・バウンスも面倒を見る。
    ///
    /// ```ignore
    /// div().scroll("rows").flex_1().flex_col().children(rows)
    /// ```
    ///
    /// **`id` を引数で要求するのは、 スクロール状態のキーが要るから。** 以前の
    /// `.overflow_scroll()` は id を省けて、 省いた場合はツリー上の位置
    /// (`__scroll:0.2.1`) から id を合成していた。 が、 その id は**兄弟が 1 つ
    /// 増減しただけで変わる**ので、 条件付きレンダリングでヘッダが出入りすると
    /// 別の状態を引いてスクロール位置が 0 に飛んだ (issue #14)。 安定した名前を
    /// 書かせる方に倒した。
    ///
    /// `id` はこの要素の [`Element::id`] そのもの。 `on_click` などのルーティングに
    /// 使う id と共用になるので、 別々の名前を付けることはできない。
    ///
    /// 位置をアプリ側で持ちたい場合は [`Element::scroll_manual`]。
    pub fn scroll(mut self, id: impl Into<String>) -> Self {
        self.style.overflow = Overflow::Scroll;
        self.style.scroll_owner = ScrollOwner::Runtime;
        self.id = Some(id.into());
        self
    }

    /// スクロールコンテナにするが、 位置は**アプリが持つ**。 ランタイムは
    /// `scroll_x` / `scroll_y` に一切触れない。
    ///
    /// ```ignore
    /// div().scroll_manual(0.0, self.sidebar_scroll).flex_col().children(items)
    /// ```
    ///
    /// ホイールは届かないので、 `DeclarativeApp::on_scroll_xy` などで受けて自分で
    /// 値を進めること。 慣性やバウンスも自前になる。 仮想リストのように
    /// 「行の描画自体をオフセットから決める」 実装向け。
    ///
    /// 大半の用途では [`Element::scroll`] の方が短く、 挙動も揃う。
    pub fn scroll_manual(mut self, x: f32, y: f32) -> Self {
        self.style.overflow = Overflow::Scroll;
        self.style.scroll_owner = ScrollOwner::App;
        self.style.scroll_x = x;
        self.style.scroll_y = y;
        self
    }

    /// Draw a scrollbar thumb in `thumb` color at this scroll container's
    /// right edge whenever its content overflows vertically. Only meaningful
    /// together with `.scroll(id)` / `.scroll_manual(x, y)`. Indicator only (not draggable);
    /// hidden while the content fits the viewport.
    pub fn scrollbar(mut self, thumb: Color) -> Self {
        self.style.scrollbar_thumb = Some(thumb);
        self
    }

    /// Set position type.
    pub fn position(mut self, p: Position) -> Self {
        self.style.position = p;
        self
    }

    /// Set position to absolute.
    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    /// Set absolute position (top, left) in pixels. Implies absolute positioning.
    pub fn pos(mut self, left: f32, top: f32) -> Self {
        self.style.position = Position::Absolute;
        self.style.inset_left = Dimension::Px(left);
        self.style.inset_top = Dimension::Px(top);
        self
    }

    /// Set a visual-only X translation in px. Layout (taffy) is unaffected;
    /// the element's render rect and hit region are simply shifted, so
    /// siblings stay in place. Designed for spring-driven hover anims.
    pub fn tx(mut self, x: f32) -> Self {
        self.style.translate_x = x;
        self
    }

    /// Set a visual-only Y translation in px. See [`Element::tx`].
    pub fn ty(mut self, y: f32) -> Self {
        self.style.translate_y = y;
        self
    }

    /// Set both X and Y visual translation in px. See [`Element::tx`].
    pub fn xlate(mut self, x: f32, y: f32) -> Self {
        self.style.translate_x = x;
        self.style.translate_y = y;
        self
    }

    /// Set a visual-only uniform scale about the element's center. Layout is
    /// unaffected (siblings stay put) and the whole subtree scales with it —
    /// see [`ElementStyle::scale`]. `1.0` = no scaling.
    pub fn scaled(mut self, factor: f32) -> Self {
        self.style.scale = factor;
        self
    }

    // -- Text --

    /// Set text color.
    pub fn color(mut self, c: Color) -> Self {
        self.style.color = c;
        self
    }

    /// Set font size (pixels).
    pub fn font_size(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self
    }

    /// Set bold.
    pub fn bold(mut self) -> Self {
        self.style.bold = true;
        self
    }

    /// Set an explicit font weight (100–900), overriding `bold`. Common values:
    /// 300 light, 400 regular, 500 medium, 600 semibold, 700 bold. The face must
    /// provide the weight (system sans/mono do); otherwise cosmic-text picks the
    /// nearest available.
    pub fn font_weight(mut self, weight: u16) -> Self {
        self.style.font_weight = Some(weight);
        self
    }

    /// Add inter-glyph tracking in logical px. Positive spreads letters
    /// (spaced eyebrows / caps), negative tightens (large display headings).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.style.letter_spacing = px;
        self
    }

    /// Set line height as a multiple of the font size (CSS unitless
    /// `line-height`). `1.0` is snug; `1.5`–`1.6` is comfortable body copy.
    /// Unset (default) = 1.4.
    pub fn line_height(mut self, mult: f32) -> Self {
        self.style.line_height = Some(mult);
        self
    }

    /// Use monospace font.
    pub fn mono(mut self) -> Self {
        self.style.monospace = true;
        self
    }

    /// 斜体にする。 face に斜体が無ければ cosmic-text が傾けて代用する
    /// (合成斜体)。 日本語フォントは斜体を持たないのが普通なので、 和文は
    /// ほぼ合成になる。
    pub fn italic(mut self) -> Self {
        self.style.italic = true;
        self
    }

    /// 折り返した行の揃え方 (CSS `text-align`)。
    ///
    /// **効くのは折り返しが起きたときだけ**。 1 行しか無いテキスト要素は箱が
    /// 中身ぴったりなので揃える余白が無く、 見た目は変わらない。 その場合は
    /// 幅を与える (`.w_full()`) か、 親の [`Element::justify_center`] を使う。
    pub fn text_align(mut self, a: TextAlign) -> Self {
        self.style.text_align = a;
        self
    }

    /// Shortcut: `text-align: center`。 折り返す前提の段落向け。
    pub fn text_center(self) -> Self {
        self.text_align(TextAlign::Center)
    }

    /// Shortcut: `text-align: end` (日本語 / 英語なら右)。
    pub fn text_right(self) -> Self {
        self.text_align(TextAlign::End)
    }

    /// Shape this element's text in a specific named font family, overriding
    /// the generic (and any app-preferred) family. The face must be loaded in
    /// the renderer's font DB (system fonts are by default); an unknown name
    /// falls back through cosmic-text's normal fallback chain. Word-style font
    /// pickers use this to preview each row in its own face.
    pub fn font_family(mut self, name: impl Into<String>) -> Self {
        self.style.font_family = Some(name.into());
        self
    }

    /// Clamp wrapped text to at most `n` lines. The text wraps
    /// naturally within the element's computed width; if the
    /// wrapped output would exceed `n` lines, the last visible line
    /// gets truncated and `…` appended. Pass `0` to remove the
    /// clamp and restore unlimited wrap (the default).
    ///
    /// Most useful on `text(...)` elements inside a sized card
    /// where you want a preview that grows up to a known cap:
    ///
    /// ```ignore
    /// text(long_string).max_lines(2)  // up to 2 lines, then "…"
    /// ```
    pub fn max_lines(mut self, n: u32) -> Self {
        self.style.max_lines = if n == 0 { None } else { Some(n) };
        self
    }

    /// Paint background highlight rects behind the glyphs of the given byte
    /// ranges — Word-style find-in-page highlight. The ranges and colors are
    /// carried in [`HighlightSpec`]; the runtime resolves them to per-line
    /// rects using the same glyph hitboxes the text-selection system uses, so
    /// they track wrapping and CJK shaping exactly. Drawn below the text and
    /// above the element background. No-op on elements without text.
    ///
    /// ```ignore
    /// text(body).highlight(HighlightSpec {
    ///     ranges: vec![(4, 6), (20, 22)],
    ///     color: hit_bg,
    ///     current: Some(0),
    ///     current_color: active_bg,
    /// })
    /// ```
    ///
    /// **Calls accumulate.** One [`HighlightSpec`] paints one background color
    /// across its ranges (plus a second color for the single `current` range),
    /// which is all find-in-page needs. Text that carries two independent
    /// colorings at once — a new/old comparison where deletions are red and
    /// insertions are green, interleaved through one sentence — needs one spec
    /// per coloring:
    ///
    /// ```ignore
    /// text(body)
    ///     .highlight(HighlightSpec { ranges: deleted,  color: del_bg, ..Default::default() })
    ///     .highlight(HighlightSpec { ranges: inserted, color: add_bg, ..Default::default() })
    /// ```
    ///
    /// Specs paint in call order, so a later one covers an earlier one where
    /// their ranges overlap. Keeping the text a single element — rather than
    /// splitting it into one element per colored run — is the point: wrapping
    /// and CJK shaping stay intact because the runtime only lays rects behind
    /// glyphs it already positioned.
    pub fn highlight(mut self, spec: HighlightSpec) -> Self {
        self.style.highlight.push(spec);
        self
    }

    /// Mark byte ranges of this text element as clickable + hoverable in-body
    /// links. A pointer press inside a range dispatches `on_click(id)`; a
    /// `tooltip` shows on hover; the range is underlined in its `color`. The
    /// runtime resolves ranges to rects via the same glyph hitboxes as
    /// find-in-page highlight, so links track wrapping/CJK shaping. Used for
    /// 条文リンク in flowing 本文. No-op on elements without text.
    pub fn link_ranges(mut self, ranges: Vec<LinkRange>) -> Self {
        self.style.link_ranges = Some(ranges);
        self
    }

    // -- Button-specific --

    /// Set accent color (button background).
    pub fn accent(mut self, color: Color) -> Self {
        if let ElementKind::Button { ref mut accent, .. } = self.kind {
            *accent = Some(color);
        }
        // Also set as background for convenience
        self.style.background = color;
        self
    }

    // -- Identity --

    /// Set element ID for event dispatch (click/hover identification).
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the mouse cursor shown while hovering this element. Use
    /// `Cursor::Pointer` for buttons / clickable tiles; leave unset to
    /// inherit from ancestors / fall back to the platform default.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Opt this element and its entire subtree out of text selection —
    /// the CSS `user-select: none` equivalent.
    ///
    /// Put it on chrome (toolbars, sidebars, table headers, status bars) so
    /// that dragging inside a panel doesn't smear a selection highlight over
    /// every label. It inherits, so one call on the panel root covers the
    /// whole panel; prose stays selectable because you don't set it there.
    ///
    /// To kill selection app-wide instead, return `false` from
    /// `DeclarativeApp::text_selection_enabled`.
    pub fn no_select(mut self) -> Self {
        self.no_select = true;
        self
    }

    // -- Arc parameters (no-op on non-Arc elements) --

    /// Set the active fill fraction `[0, 1]` of an arc element.
    pub fn arc_value(mut self, v: f32) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.value = v.clamp(0.0, 1.0);
        }
        self
    }

    /// Set the donut band thickness (logical px) of an arc element.
    pub fn arc_thickness(mut self, t: f32) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.thickness = t.max(0.0);
        }
        self
    }

    /// Set both fill (active) and track (inactive) colors of an arc.
    pub fn arc_colors(mut self, fill: Color, track: Color) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.fill_color = fill;
            a.track_color = track;
        }
        self
    }

    /// Override the angular extent of an arc. `start` and `sweep` are
    /// in radians (screen-space convention, y grows downward). Default
    /// is `start = 0.75π` (bottom-left), `sweep = 1.5π` (270°
    /// tachometer style).
    pub fn arc_sweep(mut self, start: f32, sweep: f32) -> Self {
        if let ElementKind::Arc(a) = &mut self.kind {
            a.start_angle = start;
            a.sweep_angle = sweep.max(0.0);
        }
        self
    }

    // -- Polyline parameters (no-op on non-Polyline elements) --

    /// Set the vertices of a polyline. Points are logical px relative to
    /// the element's layout box origin.
    pub fn points(mut self, pts: impl Into<Vec<(f32, f32)>>) -> Self {
        if let ElementKind::Polyline(p) = &mut self.kind {
            p.points = pts.into();
        }
        self
    }

    /// Set the stroke width (logical px) of a polyline.
    pub fn stroke_width(mut self, w: f32) -> Self {
        if let ElementKind::Polyline(p) = &mut self.kind {
            p.width = w.max(0.0);
        }
        self
    }

    /// Set the stroke color of a polyline.
    pub fn stroke_color(mut self, c: Color) -> Self {
        if let ElementKind::Polyline(p) = &mut self.kind {
            p.color = c;
        }
        self
    }

    // -- Children --

    /// Add a single child.
    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple children.
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = Element>,
    {
        self.children.extend(children);
        self
    }

    // -- Events --

    /// Set click handler.
    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    /// **押されたときにアプリをどう変えるかを、その場に書く。**
    ///
    /// id の割り当てとハンドラの登録を 1 回の呼び出しでやる。 だから
    /// **食い違う場所が存在しない。**
    ///
    /// ```ignore
    /// // view()
    /// div().click(ctx, "save", |app: &mut App| app.saved = true)
    /// ```
    ///
    /// # なぜこれがあるか
    ///
    /// もう一方の書き方は、 `.id("save")` を置いて
    /// [`DeclarativeApp::on_click`] で文字列を突き合わせるもの:
    ///
    /// ```ignore
    /// fn view(..) { div().id("save") }
    /// fn on_click(&mut self, id: &str) {
    ///     if id == "sav" { self.saved = true; }   // ← タイプミス
    /// }
    /// ```
    ///
    /// これは**コンパイルが通り、 押しても何も起きない**。 id を書く場所と
    /// 受ける場所が離れていて、 型が繋いでいないため。 このラウンドで潰し続けた
    /// のとまったく同じ形の失敗で、 いちばん中心の経路に残っていた。
    ///
    /// `click` なら文字列は 1 回しか出てこない。 打ち間違えても、 その要素が
    /// その処理を持つという関係は保たれる。
    ///
    /// # 動的な一覧
    ///
    /// 添字は**捕まえる**。 id から数字を切り出して `parse` するより安全で短い。
    ///
    /// ```ignore
    /// rows.push(
    ///     div().click(ctx, format!("row-{i}"), move |app: &mut App| {
    ///         app.selected = Some(i);
    ///     })
    /// );
    /// ```
    ///
    /// # 型注釈
    ///
    /// 引数の `|app: &mut App|` は書く必要がある (どのアプリ型かはここからしか
    /// 分からないため)。 間違った型を書けばコンパイルエラーになる。
    ///
    /// # 併用
    ///
    /// [`DeclarativeApp::on_click`] も従来どおり呼ばれる (こちらが先)。
    /// 混在しても壊れないので、 既存のコードは触らなくてよい。
    pub fn click<A: 'static>(
        self,
        ctx: &crate::ViewContext,
        id: impl Into<String>,
        handler: impl Fn(&mut A) + 'static,
    ) -> Self {
        let id = id.into();
        ctx.register_action(
            id.clone(),
            std::rc::Rc::new(move |any: &mut dyn std::any::Any| {
                // 降ろすのはここだけ。 アプリ側に `downcast` は出てこない。
                if let Some(app) = any.downcast_mut::<A>() {
                    handler(app);
                }
            }),
        );
        self.id(id)
    }

    /// Set hover handler.
    pub fn on_hover(mut self, handler: impl FnMut() + 'static) -> Self {
        self.on_hover = Some(Box::new(handler));
        self
    }

    /// 支援技術に伝える役割を設定する。
    ///
    /// ```ignore
    /// div().role(Role::Button).label("閉じる").on_click(|| {})
    /// ```
    ///
    /// 嘘の役割は無いより悪い。 迷ったら設定しない ([`Role::Group`] 扱い)。
    pub fn role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    /// 支援技術が読み上げる名前を設定する (ARIA の `aria-label` 相当)。
    ///
    /// 中の文字がそのまま名前になるなら不要。 **アイコンだけのボタンや画像**の
    /// ように見た目から名前が取れないものに付ける。
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// 見出しにする。 `level` は 1 が最上位。
    pub fn heading(mut self, level: u8) -> Self {
        self.role = Some(Role::Heading);
        self.heading_level = Some(level);
        self
    }

    /// Mark this element as focusable.
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    // -- Hover / Active state styles --

    /// Set hover style overrides via a closure.
    /// ```ignore
    /// div().bg(surface).hover(|s| s.bg(surface_hover).scale(1.02))
    /// ```
    pub fn hover(mut self, f: impl FnOnce(StateStyle) -> StateStyle) -> Self {
        self.hover_style = Some(f(StateStyle::default()));
        self
    }

    /// Set active (pressed) style overrides via a closure.
    /// ```ignore
    /// div().bg(surface).active(|s| s.bg(surface_active).scale(0.98))
    /// ```
    pub fn active(mut self, f: impl FnOnce(StateStyle) -> StateStyle) -> Self {
        self.active_style = Some(f(StateStyle::default()));
        self
    }

    /// Mark this element as a click target. Bundles the standard
    /// affordance — pointer cursor on hover, a small scale-up + caller-
    /// supplied bg shift on hover, a tiny scale-down on active (press).
    /// No transitions: snap by default. If you want the change to
    /// animate smoothly, chain `.spring_transition(k, c)` after.
    ///
    /// `hover_bg` is left to the caller because the right hover color
    /// is a design call — destructive actions usually want a red,
    /// neutral actions a subtle surface shift.
    ///
    /// ```ignore
    /// div().id("dismiss")
    ///     .w(Px(22.0)).h(Px(22.0))
    ///     .pressable(palette.surface_container_high)
    ///     .children([text("×")])
    /// ```
    pub fn pressable(self, hover_bg: Color) -> Self {
        self.cursor(Cursor::Pointer)
            .hover(move |s| s.scale(1.1).bg(hover_bg))
            .active(|s| s.scale(0.95))
    }

    // -- Transitions --

    /// Add a transition for a specific property.
    /// ```ignore
    /// div().transition(TransitionProperty::Background, TransitionKind::Easing {
    ///     duration: 0.3,
    ///     function: EasingFn::EaseOut,
    /// })
    /// ```
    pub fn transition(mut self, property: TransitionProperty, kind: TransitionKind) -> Self {
        self.transitions.push(Transition { property, kind });
        self
    }

    /// Shorthand: transition all properties with a spring.
    /// ```ignore
    /// div().spring_transition(200.0, 20.0)
    /// ```
    pub fn spring_transition(mut self, stiffness: f32, damping: f32) -> Self {
        self.transitions.push(Transition {
            property: TransitionProperty::All,
            kind: TransitionKind::Spring { stiffness, damping },
        });
        self
    }

    /// Shorthand: transition all properties with duration-based easing.
    /// ```ignore
    /// div().ease_transition(0.3, EasingFn::EaseOut)
    /// ```
    pub fn ease_transition(mut self, duration: f32, function: EasingFn) -> Self {
        self.transitions.push(Transition {
            property: TransitionProperty::All,
            kind: TransitionKind::Easing { duration, function },
        });
        self
    }

    // -- Overlay --

    /// Mark this element (and all its children) as overlay content.
    /// Overlay elements render on top of all base-layer content,
    /// preventing text bleed-through from lower elements.
    ///
    /// Also sets `position: Absolute` with `inset: 0,0` so the overlay is
    /// taken out of the parent flex flow (dropping a context menu / popup
    /// into a `flex_col` won't push the main content around) AND anchored
    /// to the parent's top-left corner by default. Callers that want
    /// custom positioning can chain `.pos(x, y)` afterwards.
    pub fn overlay(mut self) -> Self {
        self.overlay = true;
        self.style.position = Position::Absolute;
        self.style.inset_top = Dimension::Px(0.0);
        self.style.inset_left = Dimension::Px(0.0);
        self
    }

    // -- Tooltip --

    /// Set tooltip text shown on hover after a short delay.
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    // -- Drag & Drop --

    /// Mark this element as draggable with the given payload data.
    pub fn draggable(mut self, data: impl Into<String>) -> Self {
        self.drag_data = Some(data.into());
        self
    }

    /// Mark this element as a drop zone (accepts dragged items).
    pub fn droppable(mut self) -> Self {
        self.drop_zone = true;
        self
    }

    /// Enable presence animation (fade in/out when added/removed from tree).
    /// Requires `.id()` to be set for tracking.
    pub fn animate_presence(mut self) -> Self {
        self.animate_presence = true;
        self
    }
}

// ---------------------------------------------------------------------------
// StateStyle builder methods
// ---------------------------------------------------------------------------

impl StateStyle {
    pub fn bg(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadow = Some(Some(shadow));
        self
    }

    pub fn no_shadow(mut self) -> Self {
        self.shadow = Some(None);
        self
    }

    pub fn glow(mut self, color: Color, radius: f32) -> Self {
        self.shadow = Some(Some(BoxShadow {
            color,
            offset: Point::ZERO,
            blur: radius,
            spread: 0.0,
        }));
        self
    }

    pub fn glow_sm(mut self, color: Color) -> Self {
        self.glow(color.with_alpha(0.3), 8.0)
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn rounded(mut self, radii: Corners<f32>) -> Self {
        self.corner_radius = Some(radii);
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = Some(scale);
        self
    }

    pub fn translate_x(mut self, x: f32) -> Self {
        self.translate_x = Some(x);
        self
    }

    pub fn translate_y(mut self, y: f32) -> Self {
        self.translate_y = Some(y);
        self
    }

    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = Some(x);
        self.translate_y = Some(y);
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = Some(gap);
        self
    }

    pub fn w(mut self, d: Dimension) -> Self {
        self.width = Some(d);
        self
    }

    pub fn h(mut self, d: Dimension) -> Self {
        self.height = Some(d);
        self
    }

    pub fn p(mut self, d: Dimension) -> Self {
        self.padding = Some(EdgeDimensions::all(d));
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }
}
