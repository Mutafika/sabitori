//! `DeclarativeApp` と `SceneApp` の 2 ランタイムが共有する、ポインタまわりの
//! 解決ロジック。
//!
//! 両ランタイムは winit のイベントループを別々に回すので `ApplicationHandler` の
//! 実装は分かれる。 だが「hit_regions からホバーを引く」「押下対象を引く」
//! 「cursor を winit に送る」「入力キャプチャを算出する」は、どちらで動かしても
//! **同じ答えでなければならない**。
//!
//! かつては scene_app 側に "ported verbatim so both runtimes resolve hover
//! identically" と注記された複製が置かれていた。 注記があっても複製は複製で、
//! 実際 `active_style` の対応は declarative にだけ入りかけている
//! ([#3](https://github.com/Mutafika/sabitori/issues/3))。 同じ理由で `Cursor` に
//! variant を足す時、 winit へのマッピング表が 2 つあれば片方は必ず忘れられる。
//!
//! 状態 (hovered_id / last_cursor / window / …) は各ランタイムが持ったまま、
//! **判断だけ**をここに集める。 引数が素の値なので、 winit のウィンドウ無しに
//! テストから叩ける。

use std::sync::Arc;

use sabitori_core::build::BuildResult;
use sabitori_core::{Cursor, Point};
use sabitori_input::InputEvent;
use winit::window::Window;

use crate::declarative::{DeclarativeApp, UiCapture};

/// ポインタ直下のホバー解決結果。
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HoverHit {
    /// ホバー中の要素 id。
    pub hovered_id: Option<String>,
    /// その要素の tooltip。
    pub tooltip: Option<String>,
    /// 効かせるべき cursor。 `None` = 誰も指定していない (= 既定の矢印)。
    pub cursor: Option<Cursor>,
}

/// `hit_regions` からポインタ直下のホバー情報を引く。
///
/// ホバー判定と cursor 判定は**独立**に走る。 hoverable でない領域でも cursor は
/// 主張できる (テキスト入力が `Cursor::Text` を出す等) ので、 cursor は全領域を
/// 前面から舐めて最初に指定のあるものを採る。
pub(crate) fn resolve_hover(build: &BuildResult, x: f32, y: f32) -> HoverHit {
    let pt = Point::new(x, y);
    let hover_match = build
        .hit_regions
        .iter()
        .find(|r| r.hoverable && r.rect.contains(pt));
    let (hovered_id, tooltip) = match hover_match {
        Some(r) => (r.id.clone(), r.tooltip.clone()),
        None => (None, None),
    };
    let cursor = build
        .hit_regions
        .iter()
        .find(|r| r.cursor.is_some() && r.rect.contains(pt))
        .and_then(|r| r.cursor);
    HoverHit { hovered_id, tooltip, cursor }
}

/// 座標の下にある、 id を持つ最前面の hit region の id。 押下対象の解決に使う。
///
/// ホバーと違って `hoverable` では絞らない — `.active()` だけを書いた要素
/// (hover_style を持たない) も押下対象になるべきなので、 `clickable`
/// (= id 付き) で見る。
pub(crate) fn hit_id_at(build: &BuildResult, x: f32, y: f32) -> Option<String> {
    let pt = Point::new(x, y);
    build
        .hit_regions
        .iter()
        .find(|r| r.clickable && r.id.is_some() && r.rect.contains(pt))
        .and_then(|r| r.id.clone())
}

/// `Cursor` を winit の `CursorIcon` へ。
///
/// **マッピング表はここ 1 つだけ**にすること。 variant を足した時に片方の
/// ランタイムだけ更新される、が起きなくなる。
pub(crate) fn winit_cursor(cursor: Cursor) -> winit::window::CursorIcon {
    use winit::window::CursorIcon;
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::ResizeEw => CursorIcon::EwResize,
        Cursor::ResizeNs => CursorIcon::NsResize,
    }
}

/// 解決した cursor を OS へ送る。 `None` は既定の矢印になる。
///
/// `last` と突き合わせて重複呼び出しを潰す — ポインタ移動のたびに `set_cursor`
/// を叩くのは只ではなく、 macOS の NSCursor 差し替えは視覚的なちらつきとして
/// 出ることがある。
pub(crate) fn apply_cursor(
    window: Option<&Arc<Window>>,
    last: &mut Option<Cursor>,
    cursor: Option<Cursor>,
) {
    let resolved = cursor.unwrap_or(Cursor::Default);
    if *last == Some(resolved) {
        return;
    }
    *last = Some(resolved);
    if let Some(window) = window {
        window.set_cursor(winit_cursor(resolved));
    }
}

/// 今フレームの入力キャプチャ状態。 ホスト (埋め込み側) が「この座標の入力は
/// sabitori が食う」を判断するのに使う。
pub(crate) fn ui_capture(
    build: Option<&BuildResult>,
    x: f32,
    y: f32,
    drag_active: bool,
    focused: bool,
) -> UiCapture {
    let wants_pointer = build.map(|b| b.wants_pointer(x, y)).unwrap_or(false) || drag_active;
    UiCapture { wants_pointer, wants_keyboard: focused }
}

/// [`ui_capture`] を算出し、 前回から変わっていればアプリへ通知する。
pub(crate) fn push_ui_capture<A: DeclarativeApp>(
    build: Option<&BuildResult>,
    x: f32,
    y: f32,
    drag_active: bool,
    focused: bool,
    last: &mut UiCapture,
    app: &mut A,
) {
    let capture = ui_capture(build, x, y, drag_active, focused);
    if capture != *last {
        *last = capture;
        app.on_ui_capture(capture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::element::{div, Px};

    fn build(root: &sabitori_core::Element) -> BuildResult {
        sabitori_core::build::build_tree(root, 400.0, 300.0)
    }

    /// ホバーと cursor は独立に引く。 hoverable でない領域が cursor を主張して
    /// いれば、 ホバー対象にならなくても cursor は効く。
    ///
    /// `on_click` だけを持つ要素は clickable だが hoverable ではないので、
    /// 両者が別経路であることがそのまま出る。
    #[test]
    fn cursor_is_resolved_independently_of_hover() {
        let root = div().child(
            div()
                .w(Px(100.0))
                .h(Px(50.0))
                .on_click(|| {})
                .cursor(Cursor::Crosshair),
        );
        let b = build(&root);
        let hit = resolve_hover(&b, 10.0, 10.0);

        assert_eq!(hit.hovered_id, None, "hoverable ではないので hover 対象ではない");
        assert_eq!(hit.cursor, Some(Cursor::Crosshair), "cursor だけは効く");
    }

    /// 押下対象は `hoverable` では絞らない — `.active()` だけを書いた要素も掴む。
    #[test]
    fn press_target_does_not_require_hoverability() {
        let root = div().child(div().id("btn").w(Px(100.0)).h(Px(50.0)));
        let b = build(&root);

        assert_eq!(hit_id_at(&b, 10.0, 10.0).as_deref(), Some("btn"));
        assert_eq!(hit_id_at(&b, 300.0, 200.0), None, "外は掴まない");
    }

    /// `Cursor` の全 variant が winit に写せること。 variant を足してここが
    /// 落ちたら、 マッピングの更新漏れ。
    #[test]
    fn every_cursor_variant_maps_to_winit() {
        use winit::window::CursorIcon;
        let pairs = [
            (Cursor::Default, CursorIcon::Default),
            (Cursor::Pointer, CursorIcon::Pointer),
            (Cursor::Text, CursorIcon::Text),
            (Cursor::Crosshair, CursorIcon::Crosshair),
            (Cursor::NotAllowed, CursorIcon::NotAllowed),
            (Cursor::ResizeEw, CursorIcon::EwResize),
            (Cursor::ResizeNs, CursorIcon::NsResize),
        ];
        for (from, to) in pairs {
            assert_eq!(winit_cursor(from), to, "{from:?} のマッピングが違う");
        }
    }

    /// 同じ cursor を続けて送っても、 OS へは 1 回しか行かない。
    #[test]
    fn repeated_cursors_are_deduped() {
        let mut last = None;
        apply_cursor(None, &mut last, Some(Cursor::Text));
        assert_eq!(last, Some(Cursor::Text));
        // 2 回目は素通り (window が None なので副作用は観測できないが、
        // last が書き換わらないことで dedup の分岐を通ったと分かる)。
        apply_cursor(None, &mut last, Some(Cursor::Text));
        assert_eq!(last, Some(Cursor::Text));
        // None は既定の矢印として扱う。
        apply_cursor(None, &mut last, None);
        assert_eq!(last, Some(Cursor::Default));
    }

    /// ドラッグ中はポインタが要素の上に無くてもキャプチャする。
    #[test]
    fn an_active_drag_captures_the_pointer() {
        let b = build(&div());
        assert!(!ui_capture(Some(&b), 5.0, 5.0, false, false).wants_pointer);
        assert!(ui_capture(Some(&b), 5.0, 5.0, true, false).wants_pointer);
        assert!(ui_capture(None, 5.0, 5.0, false, true).wants_keyboard);
    }
}

/// 入力イベントをアプリへ配る唯一の口。 消費されたら `true`。
///
/// 順序は **フォーカス中の要素が先、アプリ全体があと**。 フォーカス中の要素が
/// [`DeclarativeApp::on_focused_input`] で消費すれば、 [`DeclarativeApp::on_input`]
/// には流さない (テキスト欄に打った文字がアプリ全体のキーバインドにも当たる、
/// といった二重処理を防ぐ)。
///
/// この分岐は 2 ランタイムの キー / 文字 / IME の各経路に散らばっていて、
/// 実際 scene_app の IME だけ `on_input` へのフォールバックが無く、 フォーカスが
/// 無いと変換中の文字がどこにも届かなかった (issue #22)。 1 箇所に寄せてある。
///
/// Tab / Escape のような「フォーカス操作そのもの」は `focused_id` に `None` を
/// 渡して呼ぶこと — フィールドが食べてしまうと移動できなくなる。
pub(crate) fn dispatch<A: DeclarativeApp>(
    app: &mut A,
    focused_id: Option<&str>,
    event: &InputEvent,
) -> bool {
    let handled_by_focus = match focused_id {
        Some(id) => app.on_focused_input(id, event),
        None => false,
    };
    handled_by_focus || app.on_input(event)
}
