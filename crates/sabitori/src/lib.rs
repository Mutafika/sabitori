pub use sabitori_core::*;
pub use sabitori_gpu::{GpuRenderer, ImageInstance, ImageRenderer, OrbitCamera, RectInstance};
// sabitori-input の公開項目は全部ここに出す。 部分的に出すと「型は見えるのに、 その型の
// フィールドや戻り値の型が名前で書けない」状態になり、 下流はファサード越しにイベントを
// 構築できなくなる (実際 PointerKind の欠落で下流のビルドが落ちた)。 項目を足したら
// ここにも足すこと — tests/facade.rs がコンパイル時に見張っている。
pub use sabitori_input::{
    ActivePointer, InputEvent, InteractionState, Key, Modifiers, MouseButton, PointerId,
    PointerKind, PointerState, BUTTON_MIDDLE, BUTTON_PRIMARY, BUTTON_SECONDARY, MOUSE_POINTER_ID,
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
pub use sabitori_style::{
    AlignItems, BoxShadow, Dimension, DimensionExt, Display, EdgeDimensions, Fill, FlexDirection,
    FlexWrap, JustifyContent, Overflow, Position, StyleProps, Theme,
};
pub use sabitori_window::{SabitoriApp, EmbeddedRunner, run};

pub mod bridge;
pub mod declarative;
pub mod scroll_sync;
pub mod slider_sync;
pub mod image_runtime;
pub(crate) mod input_router;
pub mod scene_app;
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
