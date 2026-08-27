//! バックグラウンド起動 (`SABITORI_BACKGROUND=1`) — スクリーンショット検証用。
//!
//! GUI の自動検証 (エージェントが窓を起こしてスクリーンショットで確認する類) は、
//! 起動のたびに窓が最前面に出てフォーカスを奪い、作業中のユーザーの打鍵が
//! アプリへ流れ込む。`SABITORI_BACKGROUND=1` で起動すると (macOS):
//!
//! 1. アプリを Accessory activation policy にし、起動時にアクティベートしない
//!    (Dock に出ない・フォーカスを奪わない・キー入力が届かない)。
//! 2. 窓を**非表示のまま生成**し、`setFrameOrigin` で完全画面外へ移してから表示
//!    する。1 フレームも画面に映らないが、window server は通常どおり合成し続ける
//!    ので `screencapture -l<WID>` によるキャプチャはそのまま生きる。
//!
//! 手段の選定メモ (全部実測):
//! - **生成属性の `with_position` は使えない** — macOS は titled window の初期配置を
//!   constrainFrameRect でクランプし、画面内へ引き戻す。生成後の `setFrameOrigin`
//!   はクランプされない。
//! - **ウィンドウレベルを壁紙 (kCGDesktopWindowLevel) より下に沈めるのは使えない** —
//!   見えなくはなるが、macOS がその窓の合成を止め backing store が凍るため、
//!   キャプチャが「起動数秒時点の古いフレーム」を返し続ける。
//! - **`alphaValue = 0` も使えない** — screencapture が窓の alpha ごと合成するので
//!   真っ白な画像になる。
//!
//! macOS 以外の native では 1. 相当が無いため何もしない (env は無視)。wasm も同様。

#[cfg(not(target_arch = "wasm32"))]
use winit::event_loop::EventLoop;
use winit::window::WindowAttributes;

/// `SABITORI_BACKGROUND` が立っているか (空文字と "0" は OFF)。
pub fn background_launch() -> bool {
    matches!(std::env::var("SABITORI_BACKGROUND"), Ok(v) if !v.is_empty() && v != "0")
}

/// EventLoop を作る。バックグラウンド起動時の macOS は Accessory ポリシー +
/// activate しない設定で、既存アプリからフォーカスを奪わない。
/// 通常起動は `EventLoop::new()` と同じ。
#[cfg(not(target_arch = "wasm32"))]
pub fn build_event_loop() -> EventLoop<()> {
    #[allow(unused_mut)]
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    if background_launch() {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_activate_ignoring_other_apps(false);
    }
    builder.build().expect("Failed to create event loop")
}

/// 窓の生成属性にバックグラウンド起動ぶんを足す。macOS では非表示で生成し、
/// [`finish_background_window`] が画面外へ移してから表示する (生成の瞬間の
/// チラ見えを 1 フレームも出さないため)。
pub fn apply_background_attrs(attrs: WindowAttributes) -> WindowAttributes {
    #[cfg(target_os = "macos")]
    if background_launch() {
        return attrs.with_visible(false);
    }
    attrs
}

/// 窓の生成直後に呼ぶ。バックグラウンド起動時の macOS は窓を完全画面外へ
/// 移してから表示する。通常起動では何もしない。
#[cfg(target_os = "macos")]
pub fn finish_background_window(window: &winit::window::Window) {
    if !background_launch() {
        return;
    }
    move_fully_offscreen(window);
    window.set_visible(true);
}

#[cfg(not(target_os = "macos"))]
pub fn finish_background_window(_window: &winit::window::Window) {}

/// NSWindow を全ディスプレイの外へ (`setFrameOrigin` はクランプされない)。
/// x=20000 は Apple の最大解像度・複数枚構成でも届かない座標。
#[cfg(target_os = "macos")]
fn move_fully_offscreen(window: &winit::window::Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return;
    };
    unsafe {
        let ns_view: &objc2_app_kit::NSView = &*(h.ns_view.as_ptr() as *const objc2_app_kit::NSView);
        if let Some(ns_window) = ns_view.window() {
            ns_window.setFrameOrigin(objc2_foundation::NSPoint::new(20000.0, 1400.0));
        }
    }
}
