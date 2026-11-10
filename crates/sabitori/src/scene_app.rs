//! SceneApp: custom GPU rendering + declarative UI overlay.
//!
//! Frame order:
//!   1. app.render_scene(ctx)  — custom 3D/2D scene with depth
//!   2. Sabitori UI overlay    — view() Element tree, no depth, LoadOp::Load
//!   3. Present
//!
//! **`run_scene` は宣言的ランタイム (`run_declarative`) をそのまま回す。** 入力・部品
//! (`text_input` / `modal` / `ask_dialog` / `.click()` …)・ファイルとダイアログの答え・
//! 閉じる前の確認・別窓・テスト (`Harness`) まで同じ道を通り、違うのは主窓の描く段
//! (シーンを描いてから UI を重ねる) と、作るとき・大きさが変わったとき・生のマウスの
//! 動きだけ ([`SceneDriver`])。
//!
//! 0.58 までは、このファイルに宣言的ランタイムを手で写したものが別にあった
//! (約 1,500 行)。写しは直しが届かずにずれ、無効なボタンが押せる・キーを離したことが
//! 届かない・UI の文字が Retina で 1 倍のまま・登録した部品が黙って何もしない、
//! などになっていた。

use sabitori_gpu::{GpuContext, SceneRenderContext};
use sabitori_input::{Delivery, InputEventKind};

use crate::declarative::DeclarativeApp;

/// Extension trait for apps that combine custom GPU rendering with declarative UI.
///
/// Implement both `DeclarativeApp` (for 2D UI overlay) and `SceneApp` (for custom scene).
pub trait SceneApp: DeclarativeApp {
    /// Called once after GPU initialization.
    /// Create your custom pipelines, shaders, buffers, and bind groups here.
    fn setup(&mut self, ctx: &GpuContext);

    /// Called when the window is resized.
    /// The depth texture is already recreated by GpuRenderer before this is called.
    fn on_resize(&mut self, _ctx: &GpuContext) {}

    /// Render the custom scene. Called every frame before the UI overlay pass.
    ///
    /// The app is responsible for creating its own render pass on `ctx.encoder`,
    /// including clearing color/depth attachments and drawing custom geometry.
    fn render_scene(&mut self, ctx: &mut SceneRenderContext);

    /// Raw (unaccelerated) relative mouse motion from `DeviceEvent::MouseMotion`.
    /// Fires on every physical mouse move regardless of button state — gate on your own
    /// drag flags. Needed because macOS does NOT deliver `WindowEvent::CursorMoved` position
    /// updates while a non-primary button (middle/right) is held, so camera pan/orbit driven
    /// off `on_pointer_move` freezes mid-drag. Use this for those drags (CAD/3D-app style).
    fn on_raw_motion(&mut self, _dx: f64, _dy: f64) {}
}

/// このランタイムが [`sabitori_input::InputEvent`] の各種別をアプリへどう届けるかの宣言。
///
/// `run_scene` は宣言的ランタイムを回すので、答えは
/// [`crate::declarative::input_delivery`] と同じ (そちらを呼ぶ)。
pub fn input_delivery(kind: InputEventKind) -> Delivery {
    crate::declarative::input_delivery(kind)
}

/// `run_scene` が宣言的ランタイムに差し込む、主窓の 3D シーンの口。アプリの型を
/// 消して `AppState` に持たせるための内部の形で、中身は [`SceneApp`] の同名の口を呼ぶだけ。
pub(crate) trait SceneDriver<A> {
    /// GPU ができた直後 (深度を用意した後) に 1 回。
    fn setup(&self, app: &mut A, ctx: &GpuContext);
    /// 窓の大きさが変わった (深度はレンダラが作り直してある)。
    fn on_resize(&self, app: &mut A, ctx: &GpuContext);
    /// 毎フレーム、UI より先に描く。
    fn render_scene(&self, app: &mut A, ctx: &mut SceneRenderContext);
    /// 生の相対マウスモーション。
    fn on_raw_motion(&self, app: &mut A, dx: f64, dy: f64);
}

/// [`SceneApp`] を [`SceneDriver`] にする。
struct SceneHooks;

impl<A: SceneApp> SceneDriver<A> for SceneHooks {
    fn setup(&self, app: &mut A, ctx: &GpuContext) {
        app.setup(ctx);
    }
    fn on_resize(&self, app: &mut A, ctx: &GpuContext) {
        app.on_resize(ctx);
    }
    fn render_scene(&self, app: &mut A, ctx: &mut SceneRenderContext) {
        app.render_scene(ctx);
    }
    fn on_raw_motion(&self, app: &mut A, dx: f64, dy: f64) {
        app.on_raw_motion(dx, dy);
    }
}

/// 3D シーンの上に宣言的 UI を重ねるアプリを回す。
///
/// 中身は [`crate::run_declarative`] と同じランタイムで、主窓だけ描く段が
/// `render_scene` → UI になる。`DeclarativeApp` の口はすべて `run_declarative` と
/// 同じに効く (`titlebar()`・`extra_windows()`・`on_close_requested`・部品・…)。
pub fn run_scene<A: SceneApp + 'static>(app: A) {
    crate::declarative::run_with(app, Some(std::rc::Rc::new(SceneHooks)));
}

