pub use sabitori_core::*;
pub use sabitori_gpu::{GpuRenderer, ImageInstance, ImageRenderer, OrbitCamera, RectInstance};
// sabitori-input の公開項目は全部ここに出す。 部分的に出すと「型は見えるのに、 その型の
// フィールドや戻り値の型が名前で書けない」状態になり、 下流はファサード越しにイベントを
// 構築できなくなる (実際 PointerKind の欠落で下流のビルドが落ちた)。 項目を足したら
// ここにも足すこと — tests/facade.rs がコンパイル時に見張っている。
pub use sabitori_input::{
    ActivePointer, Delivery, InputEvent, InputEventKind, InteractionState, Key, Modifiers,
    MouseButton, PointerId, PointerKind, PointerState, BUTTON_MIDDLE, BUTTON_PRIMARY,
    BUTTON_SECONDARY, MOUSE_POINTER_ID,
};
pub use sabitori_layout::{LayoutEngine, LayoutNodeId, LayoutResult};
pub use sabitori_anim::{
    Animated, AnimationMode, ChainedAnimation, EasingFunction, Keyframe, Lerp, RepeatMode, Spring,
    TypewriterState, SpinnerState, ProgressBarState,
    GradientState, WaveState, PulseState, ColorCycleState,
    MotionState, Direction, SplashPreset,
};
pub use sabitori_scene::{NodeId, NodeStyle, NodeTree, UiNode};
pub use sabitori_text::{rotate_glyphs, GlyphInstance, TextRenderer, TextShaper, FONT_SIZE_QUANTUM};
pub use sabitori_widgets::*;
// レイアウト系の型は **core (`Element` が使う方)** をファサードの正とする。
//
// core の `element` と style の `props` は、 同じ名前の型を 9 個ずつ**別々に**
// 定義している (AlignItems / BoxShadow / Dimension / EdgeDimensions /
// FlexDirection / FlexWrap / JustifyContent / Overflow / Position)。 かつては
// ファサードが style 側だけを名前付きで出していたので、 `sabitori::Overflow` を
// import して `div().overflow(..)` に渡すと
//   expected `sabitori::element::Overflow`, found `sabitori::Overflow`
// という、 名前が同じに見えるのに型が違うエラーになった。 素直な import が
// 通らない状態だったので、 `Element` に渡せる方を無印にした。
pub use sabitori_core::element::{
    AlignItems, BoxShadow, EdgeDimensions, FlexDirection, FlexWrap, JustifyContent, Overflow,
    Position,
};
// style 側は `StyleProps` (YAML テーマ / retained な style 記述) 用。 名前が
// ぶつかるものは `Style` 接頭辞で分ける。
pub use sabitori_style::{
    AlignItems as StyleAlignItems, BoxShadow as StyleBoxShadow, Dimension as StyleDimension,
    DimensionExt as StyleDimensionExt, Display, EdgeDimensions as StyleEdgeDimensions, Fill,
    FlexDirection as StyleFlexDirection, FlexWrap as StyleFlexWrap,
    JustifyContent as StyleJustifyContent, Overflow as StyleOverflow, Position as StylePosition,
    StyleProps, Theme,
};
pub use sabitori_window::{SabitoriApp, EmbeddedRunner, run};

pub mod bridge;
pub mod declarative;
pub mod scroll_sync;
// 2 ランタイム (declarative / scene) が共有するポインタ解決。 crate 内部専用。
mod runtime_shared;
pub mod slider_sync;
pub mod image_runtime;
pub(crate) mod input_router;
pub mod scene_app;
/// アプリの回帰テストを窓も GPU も無しで書くための足場 (issue #19)。
pub mod testing;
#[cfg(target_os = "macos")]
pub mod macos_drag;
#[cfg(target_os = "macos")]
pub mod macos_blur;
#[cfg(target_os = "ios")]
pub mod ios_keyboard;
pub use declarative::{BackdropBlur, DeclarativeApp, ExtraWindow, UiCapture, run_declarative};
pub use scene_app::SceneApp;
pub use scene_app::run_scene;
pub use sabitori_gpu::{GpuContext, SceneRenderContext, UiOverlayRenderer};
