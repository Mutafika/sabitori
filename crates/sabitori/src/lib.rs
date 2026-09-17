pub use sabitori_core::*;
pub use sabitori_gpu::{GpuRenderer, ImageInstance, ImageRenderer, OrbitCamera, RectInstance};
// sabitori-input の公開項目は全部ここに出す。 部分的に出すと「型は見えるのに、 その型の
// フィールドや戻り値の型が名前で書けない」状態になり、 下流はファサード越しにイベントを
// 構築できなくなる (実際 PointerKind の欠落で下流のビルドが落ちた)。 項目を足したら
// ここにも足すこと — tests/facade.rs がコンパイル時に見張っている。
pub use sabitori_input::{
    ActivePointer, ClickCounter, Delivery, InputEvent, InputEventKind, InteractionState, Key,
    Modifiers, MouseButton, PointerId, PointerKind, PointerState, WheelPhase, BUTTON_MIDDLE,
    BUTTON_PRIMARY, BUTTON_SECONDARY, LINE_DELTA_PX, MOUSE_POINTER_ID, MULTI_CLICK_INTERVAL,
    MULTI_CLICK_SLOP, MULTI_TAP_SLOP,
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
// レイアウト系の型は core (`Element` と `StyleProps` の共通の定義) を出す。
//
// かつては core の `element` と style の `props` が同じ名前の型を 9 個**別々に**
// 定義していて、 ファサードは style 側だけを名前付きで出していた。 その結果
// `use sabitori::Overflow` した値が `div().overflow(..)` に渡せず、
//   expected `sabitori::element::Overflow`, found `sabitori::Overflow`
// という、 名前が同じに見えるのに型が違うエラーになった (issue #24)。
// 0.4.0 で style 側の重複定義を削除し、 core の 1 組に統合済み。
pub use sabitori_style::{Display, Fill, StyleProps, Theme};
pub use sabitori_window::{SabitoriApp, EmbeddedRunner, run};

pub mod bridge;
pub mod declarative;
pub mod scroll_sync;
// 2 ランタイム (declarative / scene) が共有するポインタ解決。 crate 内部専用。
mod runtime_shared;
pub mod slider_sync;
pub mod image_runtime;
pub mod hot_reload;
pub(crate) mod input_router;
pub mod scene_app;
/// システムクリップボードの読み書き (issue #20)。
pub mod clipboard;
/// ファイルを選ぶ・保存する (issue #77)。
pub mod files;
/// 非同期の結果を UI に戻す (issue #64)。
pub mod tasks;
/// アプリの回帰テストを窓も GPU も無しで書くための足場 (issue #19)。
pub mod testing;
#[cfg(target_os = "macos")]
/// 支援技術 (VoiceOver / NVDA / Orca) へツリーを渡す (#25)。native だけ。
#[cfg(not(target_arch = "wasm32"))]
mod a11y;
/// 実行時にフォントを足す (#75 の 13)。
pub mod fonts;
/// 画面外に描く (帳票・印刷・画像の書き出し) (#75 の 12)。native だけ。
#[cfg(not(target_arch = "wasm32"))]
pub mod offscreen;
pub mod macos_drag;
#[cfg(target_os = "macos")]
pub mod macos_blur;
#[cfg(target_os = "ios")]
pub mod ios_keyboard;
/// Web の文字入力の橋渡し (隠し textarea)。日本語 IME・ソフトキーボード・
/// 貼り付けが canvas だけでは届かないため (issue #73)。
#[cfg(target_arch = "wasm32")]
pub mod web_ime;
/// Web の URL と戻るボタン (History) の橋渡し (issue #74)。
#[cfg(target_arch = "wasm32")]
pub mod web_history;
/// DOM 起点の出来事でランタイムを起こす口。lazy_render が既定 true なので、
/// これが無いと積んだ入力が誰にも汲まれない (issue #73 / #74)。
#[cfg(target_arch = "wasm32")]
pub mod web_wake;
pub use declarative::{BackdropBlur, DeclarativeApp, ExtraWindow, ScrollIntent, UiCapture, run_declarative};
pub use tasks::Tasks;
pub use scene_app::SceneApp;
pub use scene_app::run_scene;
pub use sabitori_gpu::{GpuContext, SceneRenderContext, UiOverlayRenderer};
