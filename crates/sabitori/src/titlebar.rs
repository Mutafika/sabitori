//! 中身と一体のタイトルバー ([`Titlebar::Unified`]) と、窓のつかみどころ
//! ([`Element::window_drag`]) が頼む窓の操作 ([`WindowGesture`])。
//!
//! # macOS でやっていること
//!
//! Warp と同じ作り (`crates/warpui/src/platform/mac/objc/window.m` の
//! `configure_titlebar_height` と全画面の出入り、`host_view.m` の `mouseDown:` /
//! `mouseUp:`)。
//!
//! 1. 窓を作るとき、タイトルバーを透かし・文字を消し・中身を窓の上端まで広げる
//!    (`resumed` の `WindowAttributesExtMacOS`)。
//! 2. 信号ボタンを帯の縦の真ん中へ置き直す ([`install`])。タイトルバーの入れ物を
//!    帯の高さにし、本体とボタンを制約 (Auto Layout) で留める。AppKit は窓の
//!    大きさが変わるたびに入れ物を標準の高さへ戻すので、戻されたら当て直す。
//!    フルスクリーン中の信号ボタンは、画面の上端に寄せると出る OS の帯へ移る。
//!    透けたままだと中身の上にボタンだけが浮くので、その間は標準の高さ・不透明に戻す。
//! 3. 帯の空いた所を押したら窓をドラッグし、ダブルクリックならシステム設定の
//!    動作 (拡大 / しまう / 何もしない) をする ([`perform`])。押せる物 (タブ等) を
//!    押したときは窓を動かさない — どちらに当たったかはランタイムの当たり判定で決める。
//!
//! [`Element::window_drag`]: sabitori_core::element::Element::window_drag

use sabitori_core::Rect;
use winit::window::Window;

/// 窓のタイトルバーの形 ([`crate::DeclarativeApp::titlebar`])。効くのは macOS だけで、
/// 他の OS では [`Titlebar::Native`] と同じ。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Titlebar {
    /// OS 標準のタイトルバー。中身はその下から始まる。
    #[default]
    Native,
    /// タイトルバーを中身と一体にする (Warp・Safari・Finder の形)。
    ///
    /// タイトルの文字を消し、中身を窓の上端から描く。信号ボタン (閉じる・最小化・
    /// 拡大) は中身の上に重なり、上端から `height` の帯の縦の真ん中に来る。
    /// アプリは窓の上端にこの高さの帯を置き、
    ///
    /// - 部品は `ctx.window_controls` (信号ボタンの場所) の右から並べる
    /// - 帯を [`Element::window_drag`] で窓のつかみどころにする (これが無いと、
    ///   窓をつかんで動かす所が無い)
    ///
    /// ```ignore
    /// fn titlebar(&self) -> Titlebar { Titlebar::Unified { height: 38.0 } }
    ///
    /// fn view(&self, ctx: &ViewContext) -> Element {
    ///     // 信号ボタンの右から 12px 空けて並べる。フルスクリーン中などは左端から。
    ///     let lead = ctx.window_controls.map_or(12.0, |r| r.origin.x + r.size.width + 12.0);
    ///     let bar = div().window_drag().h(Px(38.0)).flex_row().items_center()
    ///         .pl(Px(lead)).children(tabs);
    ///     div().flex_col().children([bar, body])
    /// }
    /// ```
    ///
    /// [`Element::window_drag`]: sabitori_core::element::Element::window_drag
    Unified {
        /// 帯の高さ (論理 px)。信号ボタンはこの縦の真ん中に置く。
        height: f32,
    },
    /// [`Titlebar::Unified`] と同じく中身と一体にし、そのうえ信号ボタンも消す。
    /// 窓の操作ボタン (最小化・拡大・閉じる) はアプリが帯に自分で描く
    /// (Windows のように右端へ置く等)。
    ///
    /// - `ctx.window_controls` は常に `None` (帯の部品は左端から並べてよい)
    /// - ボタンは sabitori-widgets の `titlebar_buttons` で並べるのが早い
    ///   (− □ ✕ や赤・黄・緑の丸のプリセットがあり、絵も機能も差し替えられる)。
    ///   自分で描くなら、ボタンに [`Element::window_action`] を付ければランタイムが
    ///   窓に掛ける (閉じるは OS の閉じるボタンと同じ道)。アプリは窓を持たなくていい
    /// - 拡大しているか・フルスクリーンかは `ctx.window_state` で読む
    /// - フルスクリーン中だけは信号ボタンを戻す。画面の上端に寄せると出る OS の帯で
    ///   フルスクリーンを抜けられるように
    ///
    /// [`Element::window_action`]: sabitori_core::element::Element::window_action
    Custom {
        /// 帯の高さ (論理 px)。フルスクリーンを抜けたとき OS の帯をこの高さへ戻す。
        height: f32,
    },
}

impl Titlebar {
    /// 信号ボタンの左端・間隔・大きさ (論理 px)。macOS 標準の並びで、Warp と同じ値。
    const CONTROLS_LEFT: f32 = 12.0;
    const CONTROL_GAP: f32 = 6.0;
    const CONTROL_SIZE: f32 = 14.0;

    /// 帯の高さ。中身を窓の上端から描く形 (`Unified` / `Custom`) だけ。
    pub(crate) fn unified_height(self) -> Option<f32> {
        match self {
            Titlebar::Native => None,
            Titlebar::Unified { height } | Titlebar::Custom { height } => Some(height),
        }
    }

    /// この形で信号ボタンが占める場所 (論理 px、窓の左上が原点)。`Native` と、
    /// ボタンを消す `Custom` は `None`。
    ///
    /// OS もフルスクリーンも見ない。アプリが読むのは、それを畳んだ
    /// `ViewContext::window_controls`。
    pub(crate) fn window_controls(self) -> Option<Rect> {
        match self {
            Titlebar::Native | Titlebar::Custom { .. } => None,
            Titlebar::Unified { height } => Some(Rect::new(
                Self::CONTROLS_LEFT,
                ((height - Self::CONTROL_SIZE) * 0.5).max(0.0),
                Self::CONTROL_SIZE * 3.0 + Self::CONTROL_GAP * 2.0,
                Self::CONTROL_SIZE,
            )),
        }
    }
}

/// 窓のつかみどころ ([`Element::window_drag`]) への押下が頼んだ、窓そのものの操作。
/// ランタイムが押下を処理したその場で窓へ渡す。テストでは
/// [`crate::testing::Harness::take_window_gesture`] で読む。
///
/// [`Element::window_drag`]: sabitori_core::element::Element::window_drag
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowGesture {
    /// 窓をつかんで動かす。
    Drag,
    /// ダブルクリック。macOS はシステム設定の「ウインドウのタイトルバーをダブル
    /// クリックで」に従い (拡大 / しまう / 何もしない)、他の OS は最大化を切り替える。
    DoubleClick,
    /// 窓の縁・角の持ち手 ([`crate::DeclarativeApp::resize_handles`]) を押した。
    /// その縁から窓の大きさを変える。
    Resize(crate::ResizeEdge),
}

/// `gesture` を窓に掛ける。**押下を処理しているその場で**呼ぶこと — macOS の
/// ドラッグは「今処理中の押下」を OS に渡して始める (`Window::drag_window`)。
pub(crate) fn perform(window: &Window, gesture: WindowGesture) {
    match gesture {
        // 未対応の環境 (web 等) は Err を返すだけ。押下は既に引き取ったので何も起きない。
        WindowGesture::Drag => {
            let _ = window.drag_window();
        }
        WindowGesture::DoubleClick => double_click(window),
        // macOS は OS に頼めないので、ランタイムが押した所から自分で引っぱる
        // (`window_resize::begin`)。ここに来るのはそれ以外の OS。
        #[cfg(not(target_os = "macos"))]
        WindowGesture::Resize(edge) => crate::window_resize::start_os_resize(window, edge),
        #[cfg(target_os = "macos")]
        WindowGesture::Resize(_) => {}
    }
}

/// 窓の今の状態 ([`sabitori_core::ViewContext::window_state`])。
pub(crate) fn window_state(window: &Window) -> sabitori_core::WindowState {
    let mut state = sabitori_core::WindowState::default();
    state.fullscreen = window.fullscreen().is_some();
    state.maximized = is_maximized(window);
    state
}

/// 拡大しているか。macOS は winit の `is_maximized` (= `isZoomed`) を当てにしない —
/// [`macos::is_zoomed`] を参照。
fn is_maximized(window: &Window) -> bool {
    // フルスクリーンの枠も画面を埋めるので、埋め方では見ない (OS の答えのまま)。
    #[cfg(target_os = "macos")]
    if let (None, Some(ns_window)) = (window.fullscreen(), macos::ns_window(window)) {
        return macos::is_zoomed(&ns_window);
    }
    window.is_maximized()
}

/// 拡大と元に戻すを行き来する。macOS は緑のボタンと同じ `zoom:` を直に送る —
/// winit の `set_maximized` は「今拡大しているか」を `isZoomed` で決めて覚えるので、
/// それが外れる画面 ([`macos::is_zoomed`]) では覚えた値がずれ、フルスクリーンを
/// 抜けたときに勝手に拡大し直す。
fn toggle_maximized(window: &Window) {
    #[cfg(target_os = "macos")]
    if let Some(ns_window) = macos::ns_window(window) {
        macos::zoom(&ns_window);
        return;
    }
    window.set_maximized(!window.is_maximized());
}

/// 窓の操作ボタン ([`Element::window_action`]) が頼んだことを窓に掛ける。閉じる
/// ([`WindowAction::Close`]) はここでは何もしない — 窓を落とすのはランタイム
/// (OS の閉じるボタンと同じ道)。
///
/// [`Element::window_action`]: sabitori_core::element::Element::window_action
pub(crate) fn perform_action(window: &Window, action: sabitori_core::WindowAction) {
    use sabitori_core::WindowAction;
    match action {
        WindowAction::Minimize => window.set_minimized(true),
        WindowAction::ToggleMaximize => toggle_maximized(window),
        // macOS はこれで OS のフルスクリーン (専用の操作スペース) に入る。
        WindowAction::ToggleFullscreen => window.set_fullscreen(match window.fullscreen() {
            Some(_) => None,
            None => Some(winit::window::Fullscreen::Borderless(None)),
        }),
        WindowAction::Close => {}
    }
}

/// winit の窓の `NSWindow`。
#[cfg(target_os = "macos")]
pub(crate) fn ns_window(window: &Window) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    macos::ns_window(window)
}

#[cfg(target_os = "macos")]
fn double_click(window: &Window) {
    if let Some(ns_window) = macos::ns_window(window) {
        macos::double_click(&ns_window);
    }
}

#[cfg(not(target_os = "macos"))]
fn double_click(window: &Window) {
    window.set_maximized(!window.is_maximized());
}

/// 信号ボタンを高さ `height` の帯の縦の真ん中へ置き、そのまま保たせる
/// ([`Titlebar::Unified`])。`Custom` ならボタンを消す (フルスクリーン中だけ戻す)。
/// 窓を作った直後に 1 度だけ呼ぶ。
#[cfg(target_os = "macos")]
pub(crate) fn install(window: &Window, titlebar: Titlebar) {
    let Some(height) = titlebar.unified_height() else {
        return;
    };
    let hide = matches!(titlebar, Titlebar::Custom { .. });
    if let Some(ns_window) = macos::ns_window(window) {
        macos::install(ns_window, height as f64, hide);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::{NSView, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{NSRect, NSString};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    /// macOS 標準のタイトルバーの高さ。フルスクリーン中の OS の帯はこれに戻す。
    const DEFAULT_TITLEBAR_HEIGHT: f64 = 28.0;

    /// `NSWindowButton` の値 (<AppKit/NSWindow.h>)。閉じる・しまう・拡大の順。
    const BUTTONS: [usize; 3] = [0, 1, 2];
    /// 信号ボタンの左端・間隔・大きさ (`Titlebar` と同じ値)。
    const LEFT: f64 = 12.0;
    const GAP: f64 = 6.0;
    const SIZE: f64 = 14.0;
    /// 縦の真ん中からのずらし。Warp と同じ値。
    const CENTER_NUDGE: f64 = 1.0;

    /// 自分で足した制約の目印。2 度目からは足さずに値だけ変える。
    const BAR_HEIGHT_ID: &str = "sabitori.titlebar.height";
    const BUTTON_SIZE_ID: &str = "sabitori.titlebar.button";

    pub(super) fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return None;
        };
        // SAFETY: AppKit の窓の取っ手は生きている `NSView` を指す。主スレッドから呼ぶ。
        let ns_view: &NSView = unsafe { &*(h.ns_view.as_ptr() as *const NSView) };
        ns_view.window()
    }

    /// `hide` なら信号ボタンを消す ([`super::Titlebar::Custom`])。フルスクリーン中は戻す。
    pub(super) fn install(window: Retained<NSWindow>, height: f64, hide: bool) {
        apply(&window, height, hide);
        // SAFETY: 通知の登録は主スレッドから。止めない (主窓はアプリと同じだけ生きる)。
        unsafe {
            let Some(container) = titlebar_container(&window) else {
                return;
            };
            // AppKit は窓の大きさが変わるなどのたびに、入れ物を標準の高さへ戻す。
            // 戻されたら当て直す。組み直しの最中に制約を触らないよう、次の周回で
            // (主キュー)。フルスクリーン中は OS の帯なので触らない。
            let _: () = msg_send![container, setPostsFrameChangedNotifications: true];
            let w = window.clone();
            observe("NSViewFrameDidChangeNotification", container, true, move || {
                if !is_fullscreen(&w) {
                    apply(&w, height, hide);
                }
            });
            // フルスクリーンの出入り。入る前に標準の高さ・不透明へ (消したボタンも
            // 戻す — OS の帯から抜けられるように)、出る前に元へ。
            let object = Retained::as_ptr(&window) as *mut AnyObject;
            let w = window.clone();
            observe("NSWindowWillEnterFullScreenNotification", object, false, move || {
                w.setTitlebarAppearsTransparent(false);
                apply(&w, DEFAULT_TITLEBAR_HEIGHT, false);
            });
            let w = window.clone();
            observe("NSWindowWillExitFullScreenNotification", object, false, move || {
                w.setTitlebarAppearsTransparent(true);
                apply(&w, height, hide);
            });
        }
    }

    fn is_fullscreen(window: &NSWindow) -> bool {
        window.styleMask().contains(NSWindowStyleMask::FullScreen)
    }

    /// 信号ボタン → タイトルバー本体 → その入れ物。AppKit の窓の作りそのもの
    /// (Warp も同じ辿り方)。
    unsafe fn titlebar_container(window: &NSWindow) -> Option<*mut AnyObject> {
        // SAFETY: 呼び手が主スレッドから、生きている AppKit の物を渡す。
        unsafe {
            let close: *mut AnyObject = msg_send![window, standardWindowButton: BUTTONS[0]];
            if close.is_null() {
                return None;
            }
            let bar: *mut AnyObject = msg_send![close, superview];
            if bar.is_null() {
                return None;
            }
            let container: *mut AnyObject = msg_send![bar, superview];
            (!container.is_null()).then_some(container)
        }
    }

    /// タイトルバーの入れ物を高さ `height` にして窓の上端へ寄せ、信号ボタンを
    /// その縦の真ん中に留める (`hide` なら消す)。何度呼んでもよい。
    fn apply(window: &NSWindow, height: f64, hide: bool) {
        // SAFETY: 辿った view がどれか欠けていたら何もしない。主スレッドから呼ぶ。
        unsafe {
            let Some(container) = titlebar_container(window) else {
                return;
            };
            let subviews: *mut AnyObject = msg_send![container, subviews];
            let bar: *mut AnyObject = msg_send![subviews, firstObject];
            if bar.is_null() {
                return;
            }

            // 入れ物: 高さを帯に合わせ、窓の上端へ (AppKit の座標は左下が原点)。
            // 既にそうなら触らない — 触ると大きさの変化の知らせがまた来て、当て直しが
            // 止まらなくなる。
            let mut frame: NSRect = msg_send![container, frame];
            let top = window.frame().size.height - height;
            if (frame.size.height - height).abs() > 0.5 || (frame.origin.y - top).abs() > 0.5 {
                frame.size.height = height;
                frame.origin.y = top;
                let _: () = msg_send![container, setFrame: frame];
            }

            // 本体: 高さを帯に固定し、入れ物の上・左右に留める。AppKit が入れ物を
            // 組み直しても、本体はこの制約で帯の高さを保つ。
            let fresh = {
                let existing = constraint_with_id(bar, BAR_HEIGHT_ID);
                if existing.is_null() {
                    let _: () = msg_send![bar, setTranslatesAutoresizingMaskIntoConstraints: false];
                    let anchor: *mut AnyObject = msg_send![bar, heightAnchor];
                    let c: *mut AnyObject = msg_send![anchor, constraintEqualToConstant: height];
                    activate(c, Some(BAR_HEIGHT_ID));
                    let (a, b): (*mut AnyObject, *mut AnyObject) =
                        (msg_send![bar, topAnchor], msg_send![container, topAnchor]);
                    pin(a, b, 0.0);
                    let (a, b): (*mut AnyObject, *mut AnyObject) =
                        (msg_send![bar, leadingAnchor], msg_send![container, leadingAnchor]);
                    pin(a, b, 0.0);
                    let (a, b): (*mut AnyObject, *mut AnyObject) =
                        (msg_send![bar, trailingAnchor], msg_send![container, trailingAnchor]);
                    pin(a, b, 0.0);
                    true
                } else {
                    let _: () = msg_send![existing, setConstant: height];
                    false
                }
            };

            // 信号ボタン: 大きさ・左からの位置・縦の真ん中を留める。本体を留め直した
            // とき (= 本体が作り直された) と、ボタンが作り直されて目印が無いときだけ。
            for (i, which) in BUTTONS.into_iter().enumerate() {
                let button: *mut AnyObject = msg_send![window, standardWindowButton: which];
                if !button.is_null() {
                    let _: () = msg_send![button, setHidden: hide];
                }
                if button.is_null() || (!fresh && !constraint_with_id(button, BUTTON_SIZE_ID).is_null()) {
                    continue;
                }
                let _: () = msg_send![button, setTranslatesAutoresizingMaskIntoConstraints: false];
                let w: *mut AnyObject = msg_send![button, widthAnchor];
                let h: *mut AnyObject = msg_send![button, heightAnchor];
                let cw: *mut AnyObject = msg_send![w, constraintEqualToConstant: SIZE];
                let ch: *mut AnyObject = msg_send![h, constraintEqualToConstant: SIZE];
                activate(cw, Some(BUTTON_SIZE_ID));
                activate(ch, None);
                let (a, b): (*mut AnyObject, *mut AnyObject) =
                    (msg_send![button, leadingAnchor], msg_send![bar, leadingAnchor]);
                pin(a, b, LEFT + i as f64 * (SIZE + GAP));
                let (a, b): (*mut AnyObject, *mut AnyObject) =
                    (msg_send![button, centerYAnchor], msg_send![bar, centerYAnchor]);
                pin(a, b, CENTER_NUDGE);
            }
        }
    }

    /// 拡大しているか。`isZoomed` に加えて、画面の使える所 (`visibleFrame`) を窓が
    /// 埋めているかも見る。
    ///
    /// メニューバーを自動で隠す画面では `visibleFrame` がメニューバーの所まで含む。
    /// `zoom:` は一度そこまで広げるが、OS が窓をメニューバーの下へ押し込むので、
    /// 拡大したままなのに枠が「標準の拡大の枠」と食い違い、`isZoomed` が `false` に
    /// 戻る (macOS 26 で確認。5120×2160 の画面で、窓は 2160 → 2136 に縮められた)。
    pub(super) fn is_zoomed(window: &NSWindow) -> bool {
        if window.isZoomed() {
            return true;
        }
        let Some(screen) = window.screen() else {
            return false;
        };
        let (frame, visible) = (window.frame(), screen.visibleFrame());
        // メニューバーの高さぶんまでは足りなくてよい (ノッチのある画面は 37pt 前後)。
        const MENU_BAR_SLACK: f64 = 40.0;
        let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
        near(frame.origin.x, visible.origin.x)
            && near(frame.size.width, visible.size.width)
            && near(frame.origin.y, visible.origin.y)
            && frame.size.height <= visible.size.height + 1.0
            && frame.size.height >= visible.size.height - MENU_BAR_SLACK
    }

    std::thread_local! {
        /// 拡大する前の枠 (窓番号ごと)。[`zoom`] が元に戻すのに使う。
        static UNZOOMED: std::cell::RefCell<std::collections::HashMap<isize, NSRect>> =
            std::cell::RefCell::default();
    }

    /// 緑のボタンと同じ拡大 / 元に戻す。
    ///
    /// AppKit の `zoom:` は「今拡大しているか」を `isZoomed` で決める。それが外れる画面
    /// ([`is_zoomed`]) では、拡大した窓に送っても**もう一度拡大するだけ**で戻らない
    /// (OS がまたメニューバーの下へ押し込むので、見た目は何も起きない)。だから拡大する
    /// 前の枠を覚えておき、AppKit が拡大に気付いていないときはそこへ自分で戻す。
    pub(super) fn zoom(window: &NSWindow) {
        let number = window.windowNumber();
        if is_zoomed(window) {
            let saved = UNZOOMED.with(|m| m.borrow_mut().remove(&number));
            if !window.isZoomed() {
                // 覚えていない (OS の操作で拡大した) なら、画面の真ん中に 2/3 の大きさで。
                let frame = saved.unwrap_or_else(|| centered(window));
                window.setFrame_display_animate(frame, true, true);
                return;
            }
        } else {
            UNZOOMED.with(|m| m.borrow_mut().insert(number, window.frame()));
        }
        let nil: Option<&AnyObject> = None;
        // SAFETY: 送り手 (sender) に nil を取る NSWindow の操作。
        unsafe {
            let _: () = msg_send![window, zoom: nil];
        }
    }

    /// 画面の使える所の真ん中に、その 2/3 の大きさ (最小の大きさは下回らない)。
    fn centered(window: &NSWindow) -> NSRect {
        let visible = window.screen().map_or(window.frame(), |s| s.visibleFrame());
        let min = window.minSize();
        let w = (visible.size.width * 2.0 / 3.0).max(min.width);
        let h = (visible.size.height * 2.0 / 3.0).max(min.height);
        NSRect::new(
            objc2_foundation::NSPoint::new(
                visible.origin.x + (visible.size.width - w) * 0.5,
                visible.origin.y + (visible.size.height - h) * 0.5,
            ),
            objc2_foundation::NSSize::new(w, h),
        )
    }

    /// システム設定の「ウインドウのタイトルバーをダブルクリックで」に従う。
    /// 一度も設定を触っていなければ値が無く、そのときの既定は拡大 (Warp と同じ扱い)。
    pub(super) fn double_click(window: &NSWindow) {
        let Some(defaults_class) = AnyClass::get(c"NSUserDefaults") else {
            return;
        };
        // SAFETY: NSUserDefaults の標準の読み方。`stringForKey:` は値が無ければ nil。
        let action: Option<Retained<NSString>> = unsafe {
            let defaults: *mut AnyObject = msg_send![defaults_class, standardUserDefaults];
            if defaults.is_null() {
                return;
            }
            let key = NSString::from_str("AppleActionOnDoubleClick");
            msg_send![defaults, stringForKey: &*key]
        };
        let nil: Option<&AnyObject> = None;
        // SAFETY: どちらも送り手 (sender) に nil を取る NSWindow の操作。
        unsafe {
            match action.map(|s| s.to_string()).as_deref() {
                Some("Minimize") => {
                    let _: () = msg_send![window, performMiniaturize: nil];
                }
                Some("None") => {}
                // "Maximize" (拡大)・"Fill" (画面いっぱい)・未設定 は拡大で受ける。
                // 窓のボタンの拡大と同じ道 (`isZoomed` が外れる画面でも戻せる)。
                _ => zoom(window),
            }
        }
    }

    /// `name` の通知を `object` から受けたら `f` を呼ぶ。`deferred` なら主キューで
    /// 次の周回に、そうでなければ知らせたその場で。登録は解かない。
    unsafe fn observe(name: &str, object: *mut AnyObject, deferred: bool, f: impl Fn() + 'static) {
        // SAFETY: 呼び手が主スレッドから、生きている AppKit の物を渡す。
        unsafe {
            let Some(center_class) = AnyClass::get(c"NSNotificationCenter") else {
                return;
            };
            let center: *mut AnyObject = msg_send![center_class, defaultCenter];
            let queue: *mut AnyObject = match (deferred, AnyClass::get(c"NSOperationQueue")) {
                (true, Some(q)) => msg_send![q, mainQueue],
                _ => std::ptr::null_mut(),
            };
            let name = NSString::from_str(name);
            let block = RcBlock::new(move |_note: NonNull<AnyObject>| f());
            // 戻り値 (登録の札) は通知センターが持ち続ける。
            let _: *mut AnyObject = msg_send![
                center,
                addObserverForName: &*name,
                object: object,
                queue: queue,
                usingBlock: &*block,
            ];
        }
    }

    /// `view` に付いている制約のうち、目印が `id` のもの。無ければ null。
    unsafe fn constraint_with_id(view: *mut AnyObject, id: &str) -> *mut AnyObject {
        // SAFETY: 呼び手が主スレッドから、生きている AppKit の物を渡す。
        unsafe {
            let constraints: *mut AnyObject = msg_send![view, constraints];
            if constraints.is_null() {
                return std::ptr::null_mut();
            }
            let n: usize = msg_send![constraints, count];
            for i in 0..n {
                let c: *mut AnyObject = msg_send![constraints, objectAtIndex: i];
                let ident: Option<Retained<NSString>> = msg_send![c, identifier];
                if ident.is_some_and(|s| s.to_string() == id) {
                    return c;
                }
            }
            std::ptr::null_mut()
        }
    }

    /// 錨 `a` を錨 `b` + `constant` に留める。
    unsafe fn pin(a: *mut AnyObject, b: *mut AnyObject, constant: f64) {
        // SAFETY: 呼び手が主スレッドから、生きている AppKit の物を渡す。
        unsafe {
            if a.is_null() || b.is_null() {
                return;
            }
            let c: *mut AnyObject = msg_send![a, constraintEqualToAnchor: b, constant: constant];
            activate(c, None);
        }
    }

    /// 制約を効かせる。`id` があれば目印を付ける ([`constraint_with_id`] で引くため)。
    unsafe fn activate(constraint: *mut AnyObject, id: Option<&str>) {
        // SAFETY: 呼び手が主スレッドから、生きている AppKit の物を渡す。
        unsafe {
            if constraint.is_null() {
                return;
            }
            if let Some(id) = id {
                let id = NSString::from_str(id);
                let _: () = msg_send![constraint, setIdentifier: &*id];
            }
            let _: () = msg_send![constraint, setActive: true];
        }
    }
}
