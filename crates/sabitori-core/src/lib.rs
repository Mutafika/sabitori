mod color;
mod geometry;
mod theme;
pub mod element;
pub mod render_list;
pub mod build;
pub mod tui;
pub mod forms;
pub mod image_cache;

pub use color::Color;
pub use geometry::{Corners, Edges, Point, Rect, Size, TextMetrics};
pub use theme::AppTheme;

// Re-export key element API items at crate root for convenience.
pub use element::{
    arc, div, text, button, image, ArcKind, Cursor, Element, ElementKind, HighlightSpec, ImageData,
    LinkRange, ObjectFit, Role, ScrollOwner, Typography,
};
pub use element::{Dimension, Px, Percent, Auto, DimensionExt};
// レイアウト基本型。 `sabitori-style::props` が別々に定義していたものを 0.4.0 で
// ここに一本化した (issue #24)。 crate root に出しておかないと、 ファサードの
// glob (`pub use sabitori_core::*`) が拾えず `sabitori::Overflow` が消える。
pub use element::{
    AlignItems, BoxShadow, EdgeDimensions, FlexDirection, FlexWrap, JustifyContent, Overflow,
    Position,
};
// grid と、 flex に足りていなかった揃え。 `Display` だけは crate root に出さない —
// `sabitori-style` にも同名の型があり、 ファサードの glob 同士がぶつかって
// `sabitori::Display` がどちらとも決まらなくなる。 `.grid()` / `.grid_cols()` が
// あるので、 利用側がこの型を名指しする理由も無い。
pub use element::{
    grid, AlignContent, AlignSelf, GridAutoFlow, GridPlacement, TextAlign, Track, TrackSize,
};
pub use element::{
    EasingFn, StateStyle, Transition, TransitionKind, TransitionProperty,
};
pub use render_list::{RenderCommand, RenderList, RectDraw, RingDraw, TextDraw, ImageDraw};
pub use build::{
    build_tree, build_tree_measured, BuildResult, CaretPos, HitRegion, ScrollMeasure, TextMeasure,
    TextShape,
};
pub use tui::{
    block, hsep, vsep, status_bar, status_segment, key_hint, BlockBuilder,
    typewriter, spinner, progress_bar, gradient_text, wave_text, easing_bar,
    scroll_container,
    context_menu, context_menu_item, menu_separator, MenuItem,
    tooltip_popup,
};
pub use forms::{
    checkbox, radio, slider, labeled_slider, dropdown_trigger,
    segment_control, numeric_input, collapsing_header, collapsing_section,
    progress_bar as form_progress_bar, labeled_progress_bar,
};

// ---------------------------------------------------------------------------
// TooltipInfo
// ---------------------------------------------------------------------------

/// Information about an active tooltip to display.
#[derive(Clone, Debug)]
pub struct TooltipInfo {
    pub text: String,
    pub x: f32,
    pub y: f32,
}

// ---------------------------------------------------------------------------
// DragInfo — active drag state
// ---------------------------------------------------------------------------

/// 画像テクスチャの GPU 側の使用状況。
///
/// 長時間起動でだんだん様子がおかしくなる、という話を調べるとき、アプリ側からは
/// 自分がどれだけ抱えているかが全く見えなかった。窓の外から `ps -o rss` を
/// サンプリングして症状の時刻と突き合わせる、という推測の域を出ない調べ方に
/// なっていた ([#47](https://github.com/Mutafika/sabitori/issues/47))。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureStats {
    /// いまテクスチャが占めている bytes。
    pub bytes: usize,
    /// いま持っているテクスチャの枚数。
    pub count: usize,
    /// 上限 (`DeclarativeApp::texture_budget_bytes`)。
    pub budget_bytes: usize,
    /// 起動からの累計追い出し数。
    pub evictions_total: u64,
    /// 直近の 1 フレームで追い出した数。
    ///
    /// **0 でないまま続くなら、予算が 1 フレームぶんの working set より小さい。**
    /// 毎フレーム捨てては入れ直しているので、絵は正しく出たまま静かに遅くなる。
    /// 追い出しは「今フレーム使われた鍵は捨てない」「収まらない時は超過を許す」と
    /// 安全側に倒れているぶん、超過が常態化していることは外に出さないと分からない。
    pub evicted_last_frame: usize,
}

/// Information about an active drag operation.
#[derive(Clone, Debug)]
pub struct DragInfo {
    /// The drag payload ID (from `.draggable("id")`).
    pub data: String,
    /// ID of the source element being dragged.
    pub source_id: Option<String>,
    /// ID of the drop zone currently under cursor (if any).
    pub over_drop_zone: Option<String>,
}

// ---------------------------------------------------------------------------
// ViewContext — passed to DeclarativeApp::view()
// ---------------------------------------------------------------------------

/// Handle the runtime hands to `ViewContext` so `image_url()` can consult the
/// shared cache and kick off fetches without the app wiring it up.
#[derive(Clone)]
pub struct ImageCtx {
    pub cache: std::sync::Arc<std::sync::Mutex<image_cache::ImageCache>>,
    /// Called synchronously by `image_url()` for URLs not yet in the cache.
    /// The closure is responsible for spawning an async fetch + decode and
    /// writing the result back into the cache when it completes.
    pub request: std::sync::Arc<dyn Fn(&str) + Send + Sync>,
}

/// Coarse responsive breakpoint bucket derived from the viewport width.
///
/// Apps `match ctx.size_class()` to reflow their layout (e.g. a 3-pane reader
/// collapsing to a single pane) instead of scattering pixel thresholds across
/// call sites. The buckets follow the common phone / tablet / desktop split;
/// the exact cut points live in [`SizeClass::COMPACT_MAX`] and
/// [`SizeClass::MEDIUM_MAX`] so an app can reason about them explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeClass {
    /// Phone portrait / narrow window — show one pane at a time. `width < 640`.
    Compact,
    /// Tablet portrait / split view — two panes fit side by side. `640 ≤ width < 1040`.
    Medium,
    /// Tablet landscape / desktop — full multi-pane layout. `width ≥ 1040`.
    Expanded,
}

impl SizeClass {
    /// Upper bound (exclusive) of [`SizeClass::Compact`], in logical points.
    pub const COMPACT_MAX: f32 = 640.0;
    /// Upper bound (exclusive) of [`SizeClass::Medium`], in logical points.
    pub const MEDIUM_MAX: f32 = 1040.0;

    /// Bucket a viewport width into a size class.
    pub fn from_width(width: f32) -> SizeClass {
        if width < Self::COMPACT_MAX {
            SizeClass::Compact
        } else if width < Self::MEDIUM_MAX {
            SizeClass::Medium
        } else {
            SizeClass::Expanded
        }
    }

    /// How many side-by-side panes this class can comfortably host (1 / 2 / 3).
    pub fn panes(self) -> u8 {
        match self {
            SizeClass::Compact => 1,
            SizeClass::Medium => 2,
            SizeClass::Expanded => 3,
        }
    }
}

/// Context passed to `DeclarativeApp::view()` each frame.
/// Contains viewport info, mouse state, and hovered element ID.
pub struct ViewContext<'a> {
    pub width: f32,
    pub height: f32,
    /// ID of the currently hovered element (if any).
    pub hovered: Option<String>,
    /// ID of the currently focused element (if any).
    pub focused: Option<String>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// Whether the Shift key is currently held.
    pub shift_held: bool,
    /// Whether the Command (Meta/Super) key is currently held.
    pub cmd_held: bool,
    /// Scroll states: id → current offsets + viewport + content extents on both axes.
    /// Use `scroll_info("id")` for convenient access.
    pub scroll_states: std::collections::HashMap<String, ScrollInfo>,
    /// Active tooltip info (if any element's tooltip is showing).
    pub tooltip: Option<TooltipInfo>,
    /// Active drag operation info (if any).
    pub drag: Option<DragInfo>,
    /// 画像テクスチャの使用状況。計測器を持たないホスト (testing harness 等) では
    /// 全部 0 になる。
    pub textures: TextureStats,
    /// Framework-level theme with semantic color names.
    pub theme: AppTheme,
    /// Presence animation progress values: id -> progress (0.0-1.0).
    /// Use `presence()` to query exit animation progress for keeping elements rendered.
    pub presence: std::collections::HashMap<String, f32>,
    /// Shared image cache + spawner. `None` until the runtime wires it up.
    pub images: Option<ImageCtx>,
    /// Advance width of one monospace cell at font-size 1.0 — i.e. the active
    /// `.mono()` face's `measure("0…", s) / n / s`. Multiply by your font px for
    /// an exact cell width (terminals, code grids tile perfectly for any face).
    /// `0.6` is a generic fallback before the runtime measures.
    pub mono_advance: f32,
    /// 実フォントでの文字送り計測。 ランタイムが自分の [`TextRenderer`] を差し込む。
    ///
    /// 直に触らず [`ViewContext::text_width`] / [`ViewContext::caret_x`] /
    /// [`ViewContext::measure`] を使うこと。 `None` になるのは計測器を持たない
    /// ホスト (レイアウトだけ回す testing harness 等) で、 その場合は
    /// [`ViewContext::mono_advance`] からの概算にフォールバックする。
    ///
    /// [`TextRenderer`]: https://docs.rs/sabitori-text
    pub measurer: Option<&'a dyn build::TextMeasure>,
    /// `view()` の最中に登録された「ランタイムに面倒を見てほしいもの」。
    /// 直に触らず [`ViewContext::register_managed`] / [`ViewContext::take_managed`]
    /// を使う。
    pub managed: std::cell::RefCell<Vec<(String, std::rc::Rc<dyn Managed>)>>,
    /// `view()` の最中に [`Element::click`] が登録したクリック処理。
    /// 直に触らず [`ViewContext::register_action`] / [`ViewContext::take_actions`] を使う。
    pub actions: std::cell::RefCell<Vec<(String, Action)>>,
}

/// **ランタイムに配線を任せるものの目印。**
///
/// `view()` の中で [`ViewContext::register_managed`] に渡すと、 ランタイムが
/// その id への入力配信・毎フレームの tick・フォーカス状態の反映を引き受ける。
/// アプリ側に書くことは何も無い。
///
/// # なぜ型を消しているか
///
/// 中身 (テキスト欄の状態など) は `sabitori-widgets` にあり、 配信するイベント型は
/// `sabitori-input` にある。 どちらも `sabitori-core` に依存しているので、 core が
/// それらを知ると循環する。 そこで core は「id と、 何かのハンドル」だけを運び、
/// **それが何であるかはランタイム (`sabitori`) が `downcast_ref` で解釈する**。
/// core は仕組みだけを持ち、 意味を持たない。
///
/// # 何のためにあるか
///
/// 0.4.0 より前は、 `text_input(..)` を `view()` に置いてもそれだけでは動かず、
/// `on_focused_input` / `tick` / `ime_cursor_area` を別途実装する必要があった。
/// 忘れると **フォーカスは入って枠も光るのに打った文字がどこにも行かない**。
/// コンパイルは通り、 パニックもせず、 ただ何も起きない。
///
/// 登録方式なら、 ウィジェットを置いた時点で配線が済む。 **書き忘れる場所が
/// 存在しない。**
pub trait Managed: std::any::Any {
    /// ランタイムが具体型へ降ろすための口。 実装は `self` を返すだけ。
    fn as_any(&self) -> &dyn std::any::Any;
}

/// クリックされたときにアプリへ加える変更。
///
/// 中身は `&mut A`（アプリ本体）を取るクロージャだが、 core は `A` を知らないので
/// `&mut dyn Any` で受けて中で降ろす。 降ろす部分は [`Element::click`] が書くので、
/// アプリ側に `downcast` は出てこない。
pub type Action = std::rc::Rc<dyn Fn(&mut dyn std::any::Any)>;

impl ViewContext<'_> {
    /// クリック時の処理を id に結びつける。 [`Element::click`] が呼ぶ。
    pub fn register_action(&self, id: impl Into<String>, action: Action) {
        let id = id.into();
        let mut list = self.actions.borrow_mut();
        if let Some(slot) = list.iter_mut().find(|(existing, _)| *existing == id) {
            slot.1 = action;
        } else {
            list.push((id, action));
        }
    }

    /// 登録されたクリック処理を取り出す (ランタイム用)。
    pub fn take_actions(&self) -> Vec<(String, Action)> {
        std::mem::take(&mut *self.actions.borrow_mut())
    }

    /// この id の面倒をランタイムに見てもらう。 ウィジェットの実装が呼ぶ想定で、
    /// アプリが直接呼ぶことは無い。
    ///
    /// 同じ id を 2 回登録した場合は後勝ち。 `view()` は毎フレーム呼ばれるので、
    /// 登録もフレームごとにやり直される (前フレームの登録は残らない)。
    pub fn register_managed(&self, id: impl Into<String>, target: std::rc::Rc<dyn Managed>) {
        let id = id.into();
        let mut list = self.managed.borrow_mut();
        if let Some(slot) = list.iter_mut().find(|(existing, _)| *existing == id) {
            slot.1 = target;
        } else {
            list.push((id, target));
        }
    }

    /// 登録されたものを取り出す (ランタイム用)。 `view()` を呼んだ後に一度だけ。
    pub fn take_managed(&self) -> Vec<(String, std::rc::Rc<dyn Managed>)> {
        std::mem::take(&mut *self.managed.borrow_mut())
    }
}

/// Scroll state snapshot for a scroll container, exposed via [`ViewContext`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollInfo {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub content_width: f32,
    pub content_height: f32,
}

impl ViewContext<'_> {
    /// 折り返しを考慮したキャレットの位置。
    ///
    /// [`ViewContext::caret_x`] の複数行版。 あちらは x しか返さないので、
    /// 折り返す欄では使えない (キャレットが 1 行目に貼り付く)。
    ///
    /// ```ignore
    /// let shape = TextShape::new(14.0).wrap(inner_width);
    /// let c = ctx.caret_pos(&state.text(), state.cursor(), shape);
    /// div().absolute().pos(c.x, c.y).w(Px(1.5)).h(Px(c.line_height))
    /// ```
    ///
    /// 計測器を持たないホストでは [`build::approx_caret`] の等幅近似に落ちる。
    /// 近似は折り返しを模さないので、 **ヘッドレスでは `\n` の論理行だけ**が
    /// 数えられる。
    pub fn caret_pos(
        &self,
        text: &str,
        byte_offset: usize,
        shape: build::TextShape<'_>,
    ) -> build::CaretPos {
        match self.measurer {
            Some(m) => m.caret_pos(text, byte_offset, shape),
            None => build::approx_caret::caret_pos(
                text,
                byte_offset,
                shape.font_size * self.mono_advance,
                shape.typo.line_height_px(shape.font_size),
            ),
        }
    }

    /// テキスト原点からの相対座標に最も近いキャレット位置のバイト添字。
    /// クリックでカーソルを置く / ドラッグで選択するのに使う。
    ///
    /// 範囲外の座標でも必ず何かを返す (上なら先頭、 下なら末尾)。
    pub fn offset_at(
        &self,
        text: &str,
        point: (f32, f32),
        shape: build::TextShape<'_>,
    ) -> usize {
        match self.measurer {
            Some(m) => m.offset_at(text, point, shape),
            None => build::approx_caret::offset_at(
                text,
                point,
                shape.font_size * self.mono_advance,
                shape.typo.line_height_px(shape.font_size),
            ),
        }
    }

    /// バイト範囲が占める矩形を**視覚行ごとに**返す。 選択範囲の塗りと、
    /// IME 変換中の下線に使う。
    pub fn range_rects(
        &self,
        text: &str,
        range: (usize, usize),
        shape: build::TextShape<'_>,
    ) -> Vec<Rect> {
        match self.measurer {
            Some(m) => m.range_rects(text, range, shape),
            None => build::approx_caret::range_rects(
                text,
                range,
                shape.font_size * self.mono_advance,
                shape.typo.line_height_px(shape.font_size),
            ),
        }
    }

    /// `text` を 1 行で描いたときの幅 (logical px)。
    ///
    /// 折り返しは考えない。 ラベル幅に合わせて箱を作る、 区切り線の長さを決める、
    /// といった「描く前に幅が要る」用途向け。
    pub fn text_width(&self, text: &str, font_size: f32, monospace: bool) -> f32 {
        self.measure(text, font_size, false, monospace, None).width
    }

    /// `text` の先頭から `byte_offset` までの幅 (logical px)。
    ///
    /// **キャレットの x 位置**がこれ。 テキスト欄のカーソルや、 選択範囲の
    /// ハイライト矩形を組むのに使う。
    ///
    /// ```ignore
    /// let x = ctx.caret_x(&self.input.text, self.input.cursor_pos, 14.0, false);
    /// // 文字の上に幅 1.5px の div を絶対配置する、など
    /// ```
    ///
    /// `byte_offset` は UTF-8 のバイト境界であること。 文字境界でない位置を
    /// 渡すと、 直前の境界まで切り詰めて計測する (panic はしない)。
    ///
    /// # 精度について
    ///
    /// 実装は `text[..byte_offset]` を単独で整形して幅を取る。 全体を整形して
    /// クラスタ単位の送りを足し込むのとは、 合字やカーニングが境界をまたぐ場合に
    /// 1px 未満ずれ得る。 UI のテキスト欄では実用上問題にならないが、 組版精度が
    /// 要る用途では避けること。
    pub fn caret_x(&self, text: &str, byte_offset: usize, font_size: f32, monospace: bool) -> f32 {
        let n = byte_offset.min(text.len());
        // 文字境界まで戻す。 `is_char_boundary` は 0 と len で必ず true。
        let mut cut = n;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        if cut == 0 {
            return 0.0;
        }
        self.text_width(&text[..cut], font_size, monospace)
    }

    /// 実フォントでの計測。 [`Self::text_width`] で足りない場合 (太字・書体指定) 用。
    ///
    /// 計測器が無いホストでは [`Self::mono_advance`] からの概算に落ちる。
    /// 概算は等幅前提なので、 プロポーショナル書体では実物とずれる。
    pub fn measure(
        &self,
        text: &str,
        font_size: f32,
        bold: bool,
        monospace: bool,
        font_family: Option<&str>,
    ) -> Size {
        match self.measurer {
            Some(m) => {
                m.measure(
                    text,
                    font_size,
                    bold,
                    monospace,
                    font_family,
                    None,
                    None,
                    Typography::default(),
                )
                .size
            }
            // 計測器なし: 等幅 1 セル分の送りで割り切る。 プロポーショナルでは
            // ずれるが、 「0 を返して無言でキャレットが動かない」 よりはまし。
            None => Size {
                width: text.chars().count() as f32 * self.mono_advance * font_size,
                height: font_size,
            },
        }
    }

    /// Get scroll info for a scroll container by ID.
    pub fn scroll_info(&self, id: &str) -> Option<ScrollInfo> {
        self.scroll_states.get(id).copied()
    }

    /// Whether the scroll container's viewport is at (or within `threshold` px of) the bottom.
    pub fn scroll_at_bottom(&self, id: &str, threshold: f32) -> bool {
        match self.scroll_info(id) {
            Some(info) => {
                let max_scroll = (info.content_height - info.viewport_height).max(0.0);
                info.scroll_y + threshold >= max_scroll
            }
            None => true, // not yet laid out — treat as bottom so first frame snaps
        }
    }

    /// Compute the visible item range for virtual scrolling.
    /// Returns (first_visible, count) given an item height.
    pub fn visible_range(&self, id: &str, item_height: f32) -> (usize, usize) {
        if let Some(info) = self.scroll_info(id) {
            let first = (info.scroll_y / item_height).floor().max(0.0) as usize;
            let count = (info.viewport_height / item_height).ceil() as usize + 2;
            (first, count)
        } else {
            (0, 100) // fallback
        }
    }

    // -- Responsive layout --

    /// The current [`SizeClass`], bucketed from [`ViewContext::width`].
    /// `match` on this to reflow the layout to the window size.
    pub fn size_class(&self) -> SizeClass {
        SizeClass::from_width(self.width)
    }

    /// Convenience: is the viewport in the single-pane [`SizeClass::Compact`] range?
    pub fn is_compact(&self) -> bool {
        self.size_class() == SizeClass::Compact
    }

    /// Convenience: is the viewport in the full multi-pane [`SizeClass::Expanded`] range?
    pub fn is_expanded(&self) -> bool {
        self.size_class() == SizeClass::Expanded
    }

    // -- Presence animation helpers --

    /// Get presence animation progress for an element (0.0 = gone, 1.0 = fully present).
    /// Useful for exit animations: keep rendering while progress > 0.
    pub fn presence(&self, id: &str) -> f32 {
        self.presence.get(id).copied().unwrap_or(0.0)
    }

    // -- Drag & Drop helpers --

    /// Whether a drag operation is currently active.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Get the drag payload data (if dragging).
    pub fn drag_data(&self) -> Option<&str> {
        self.drag.as_ref().map(|d| d.data.as_str())
    }

    /// Get the ID of the drop zone currently under the cursor (if any).
    pub fn drag_over(&self) -> Option<&str> {
        self.drag.as_ref().and_then(|d| d.over_drop_zone.as_deref())
    }

    // -- Async image loading --

    /// Returns an [`element::Element`] displaying the image at `url`. First
    /// call for a URL kicks off a background fetch + decode; subsequent
    /// calls return the decoded image once ready, and a transparent
    /// placeholder in the meantime.
    ///
    /// Chain `.w()` / `.h()` / `.rounded_px()` / `.object_fit()` etc. on
    /// the result — the returned element honours the same builder API
    /// whether it's loaded or not, so layout stays stable across loads.
    ///
    /// Panics if the runtime hasn't wired up `ImageCtx`. Won't happen for
    /// apps driven by `sabitori::run_declarative`; if you build your own
    /// driver, fall back to `element::image(key, data)` with your own
    /// cache, or populate `ViewContext::images`.
    pub fn image_url(&self, url: &str) -> element::Element {
        let images = self
            .images
            .as_ref()
            .expect("ViewContext::image_url requires a runtime that wires up ImageCtx");
        let state = images.cache.lock().unwrap().get(url);
        match state {
            image_cache::CacheState::Loaded(data) => element::image(url, data),
            image_cache::CacheState::Missing => {
                (images.request)(url);
                element::div()
            }
            image_cache::CacheState::Loading | image_cache::CacheState::Failed(_) => {
                element::div()
            }
        }
    }

    /// Raw-data variant of [`Self::image_url`]. Same fetch-on-miss behaviour,
    /// but returns `Option<ImageData>` instead of a ready-to-use `Element`.
    /// Useful for adapters (e.g., the markdown renderer's resolver callback)
    /// that need the pixel data directly.
    pub fn image_data(&self, url: &str) -> Option<element::ImageData> {
        let images = self.images.as_ref()?;
        let state = images.cache.lock().unwrap().get(url);
        match state {
            image_cache::CacheState::Loaded(data) => Some(data),
            image_cache::CacheState::Missing => {
                (images.request)(url);
                None
            }
            image_cache::CacheState::Loading | image_cache::CacheState::Failed(_) => None,
        }
    }
}

#[cfg(test)]
mod view_context_tests {
    use super::*;

    /// 1 文字 = font_size * 0.5 幅の決め打ち計測器。 実フォントに依存しないので
    /// 期待値を手で書ける。
    struct Stub;

    impl build::TextMeasure for Stub {
        fn measure(
            &self,
            content: &str,
            font_size: f32,
            _bold: bool,
            _monospace: bool,
            _font_family: Option<&str>,
            _max_width: Option<f32>,
            _max_lines: Option<u32>,
            _typo: Typography,
        ) -> TextMetrics {
            TextMetrics {
                size: Size {
                    width: content.chars().count() as f32 * font_size * 0.5,
                    height: font_size,
                },
                baseline: font_size * 0.8,
            }
        }

        fn caret_pos(&self, content: &str, byte_offset: usize, shape: build::TextShape<'_>) -> build::CaretPos {
            build::approx_caret::caret_pos(content, byte_offset, shape.font_size * 0.5, shape.font_size * 1.0)
        }

        fn offset_at(&self, content: &str, point: (f32, f32), shape: build::TextShape<'_>) -> usize {
            build::approx_caret::offset_at(content, point, shape.font_size * 0.5, shape.font_size * 1.0)
        }

        fn range_rects(
            &self,
            content: &str,
            range: (usize, usize),
            shape: build::TextShape<'_>,
        ) -> Vec<crate::Rect> {
            build::approx_caret::range_rects(content, range, shape.font_size * 0.5, shape.font_size * 1.0)
        }
    }

    fn ctx(measurer: Option<&dyn build::TextMeasure>) -> ViewContext<'_> {
        ViewContext {
            width: 800.0,
            height: 600.0,
            hovered: None,
            focused: None,
            mouse_x: 0.0,
            mouse_y: 0.0,
            shift_held: false,
            cmd_held: false,
            scroll_states: std::collections::HashMap::new(),
            tooltip: None,
            drag: None,
            textures: Default::default(),
            theme: AppTheme::default(),
            presence: std::collections::HashMap::new(),
            images: None,
            mono_advance: 0.6,
            measurer,
            managed: Default::default(),
            actions: Default::default(),
        }
    }

    /// キャレット位置が文字送りに比例して伸びること。 これが取れないと、
    /// 等幅以外のテキスト欄にカーソルを置けない (issue #15)。
    #[test]
    fn caret_x_grows_with_the_byte_offset() {
        let stub = Stub;
        let c = ctx(Some(&stub));

        assert_eq!(c.caret_x("abcd", 0, 20.0, false), 0.0, "先頭は 0");
        assert_eq!(c.caret_x("abcd", 2, 20.0, false), 20.0, "2 文字ぶん");
        assert_eq!(c.caret_x("abcd", 4, 20.0, false), 40.0, "末尾は全幅");
    }

    /// `byte_offset` が文字境界の途中でも panic しないこと。 日本語のテキスト欄では
    /// カーソル位置が 3 バイト単位で動くので、 呼び出し側の計算が 1 バイトずれた
    /// だけで落ちる API では使えない。
    #[test]
    fn caret_x_snaps_to_a_char_boundary() {
        let stub = Stub;
        let c = ctx(Some(&stub));
        let s = "あいう"; // 3 バイト x 3

        assert_eq!(c.caret_x(s, 3, 20.0, false), 10.0, "1 文字目の直後");
        // 2 バイト目 = 「あ」の途中。 直前の境界 (0) まで戻る。
        assert_eq!(c.caret_x(s, 2, 20.0, false), 0.0, "文字の途中は直前の境界へ");
        // 範囲外は末尾に丸める。
        assert_eq!(c.caret_x(s, 999, 20.0, false), 30.0, "範囲外は末尾");
    }

    /// 計測器を持たないホストでは等幅の概算に落ちること。 0 を返して黙って
    /// キャレットが動かない、という状態にはしない。
    #[test]
    fn measure_falls_back_when_no_measurer_is_wired() {
        let c = ctx(None);
        // mono_advance 0.6 x font 10 x 4 文字
        assert_eq!(c.text_width("abcd", 10.0, true), 24.0);
        assert!(c.caret_x("abcd", 2, 10.0, true) > 0.0, "概算でも動くこと");
    }
}
