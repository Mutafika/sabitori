//! 窓の縁と角の内側に置く、大きさを変える持ち手 ([`crate::DeclarativeApp::resize_handles`])。
//!
//! macOS の窓は縁の少し外側からもつかめるはずだが、中身を窓の上端まで広げた窓
//! ([`crate::Titlebar::Unified`] / [`crate::Titlebar::Custom`]) では外側がほとんど効かず、
//! 内側の細い所でしかつかめないことがある (Finder と並べると、Finder は縁に吸い付くのに
//! こちらは「近づけても変わらない」)。macOS 26 は角のつかめる所 (角を中心にした
//! 19 × 19 px) が丸い角に削られ、見えている窓の上に 4 分の 1 ほどしか残らないので、
//! 角では特に目立つ。
//!
//! 持ち手を頼んだ窓では、縁の内側の帯と下の角の内側の四角を、ランタイムが自分で
//! 大きさ変えに使う: 指が乗ったらカーソルを大きさ変えの矢印にし (下の物はホバーしない)、
//! 押したらアプリには渡さず [`WindowGesture::Resize`] として引き取る。
//!
//! - macOS: winit が大きさ変えを OS に頼めない (`drag_resize_window` が未対応) ので、
//!   押した時の窓の枠とカーソルの画面上の位置を覚え、動いた分だけ自分で枠を変える
//! - 他の OS: `drag_resize_window` で OS に渡す
//!
//! [`WindowGesture::Resize`]: crate::WindowGesture::Resize

use sabitori_core::Cursor;

/// 窓の縁と角の内側の持ち手の大きさ (論理 px)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeHandles {
    /// 縁の内側の帯の幅。macOS 自身が縁の内側に持つ幅と同じくらい (それより太いと、
    /// 縁に寄せた部品 — 一覧のスクロールの帯など — を押せる所が減る)。
    pub edge: f32,
    /// 下の 2 つの角の内側の四角の一辺。上の角は帯 (タイトルバー) の部品
    /// (閉じるボタン等) と重なりやすいので広げず、縁の帯だけにする。
    pub corner: f32,
}

impl Default for ResizeHandles {
    fn default() -> Self {
        Self {
            edge: 4.0,
            corner: 16.0,
        }
    }
}

/// つかんだ縁・角。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl ResizeEdge {
    /// 動かす辺 (左, 右, 上, 下)。
    fn sides(self) -> (bool, bool, bool, bool) {
        match self {
            ResizeEdge::North => (false, false, true, false),
            ResizeEdge::South => (false, false, false, true),
            ResizeEdge::East => (false, true, false, false),
            ResizeEdge::West => (true, false, false, false),
            ResizeEdge::NorthEast => (false, true, true, false),
            ResizeEdge::NorthWest => (true, false, true, false),
            ResizeEdge::SouthEast => (false, true, false, true),
            ResizeEdge::SouthWest => (true, false, false, true),
        }
    }

    /// 指が乗っている間のカーソル。
    pub fn cursor(self) -> Cursor {
        match self {
            ResizeEdge::North | ResizeEdge::South => Cursor::ResizeNs,
            ResizeEdge::East | ResizeEdge::West => Cursor::ResizeEw,
            ResizeEdge::NorthWest | ResizeEdge::SouthEast => Cursor::ResizeNwse,
            ResizeEdge::NorthEast | ResizeEdge::SouthWest => Cursor::ResizeNesw,
        }
    }
}

/// 窓 (`w` × `h`) の中の点 `(x, y)` (左上が原点) が当たる持ち手。
pub(crate) fn edge_at(handles: ResizeHandles, x: f32, y: f32, w: f32, h: f32) -> Option<ResizeEdge> {
    if w <= 0.0 || h <= 0.0 || x < 0.0 || y < 0.0 || x > w || y > h {
        return None;
    }
    // 下の角は広めの四角 (帯の幅より狭くはしない)
    let corner = handles.corner.max(handles.edge);
    if y >= h - corner {
        if x >= w - corner {
            return Some(ResizeEdge::SouthEast);
        }
        if x < corner {
            return Some(ResizeEdge::SouthWest);
        }
    }
    let e = handles.edge;
    let (left, right, top, bottom) = (x < e, x >= w - e, y < e, y >= h - e);
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeEdge::NorthWest),
        (_, true, true, _) => Some(ResizeEdge::NorthEast),
        (true, _, _, true) => Some(ResizeEdge::SouthWest),
        (_, true, _, true) => Some(ResizeEdge::SouthEast),
        (true, ..) => Some(ResizeEdge::West),
        (_, true, ..) => Some(ResizeEdge::East),
        (_, _, true, _) => Some(ResizeEdge::North),
        (.., true) => Some(ResizeEdge::South),
        _ => None,
    }
}

/// 押した時の枠 `start` (`[x, y, 幅, 高さ]`。AppKit の画面の座標 — 左下が原点で y は
/// 上向き) から、カーソルが `(dx, dy)` (同じ座標) 動いた時の枠。大きさは `min` 〜 `max`
/// に収め、収めて止まったら、つかんでいない側の辺は動かさない。
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(crate) fn resized_frame(
    start: [f64; 4],
    edge: ResizeEdge,
    dx: f64,
    dy: f64,
    min: (f64, f64),
    max: (f64, f64),
) -> [f64; 4] {
    let [x0, y0, w0, h0] = start;
    let (left, right, top, bottom) = edge.sides();
    let clamp = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi.max(lo));
    let w = if right {
        clamp(w0 + dx, min.0, max.0)
    } else if left {
        clamp(w0 - dx, min.0, max.0)
    } else {
        w0
    };
    let h = if top {
        clamp(h0 + dy, min.1, max.1)
    } else if bottom {
        clamp(h0 - dy, min.1, max.1)
    } else {
        h0
    };
    // 左の辺を動かしたら右の辺を、下の辺を動かしたら上の辺を留める
    let x = if left { x0 + w0 - w } else { x0 };
    let y = if bottom { y0 + h0 - h } else { y0 };
    [x, y, w, h]
}

/// 引っぱっている最中 (macOS)。押した時の枠とカーソルの位置 (どちらも画面の座標)。
#[cfg(target_os = "macos")]
pub(crate) struct Drag {
    edge: ResizeEdge,
    start: [f64; 4],
    mouse: (f64, f64),
}

/// 押した所から引っぱり始める (macOS)。窓が取れなければ `None`。
#[cfg(target_os = "macos")]
pub(crate) fn begin(window: &winit::window::Window, edge: ResizeEdge) -> Option<Drag> {
    use objc2_app_kit::NSEvent;
    let ns = crate::titlebar::ns_window(window)?;
    let f = ns.frame();
    let m = NSEvent::mouseLocation();
    Some(Drag {
        edge,
        start: [f.origin.x, f.origin.y, f.size.width, f.size.height],
        mouse: (m.x, m.y),
    })
}

/// 今のカーソルの位置に合わせて枠を変える (macOS)。
#[cfg(target_os = "macos")]
pub(crate) fn update(window: &winit::window::Window, drag: &Drag) {
    use objc2_app_kit::NSEvent;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some(ns) = crate::titlebar::ns_window(window) else {
        return;
    };
    let m = NSEvent::mouseLocation();
    // 中身を窓いっぱいに広げた窓では、中身の最小・最大がそのまま枠の最小・最大
    let (cmin, cmax) = (ns.contentMinSize(), ns.contentMaxSize());
    let (fmin, fmax) = (ns.minSize(), ns.maxSize());
    let min = (cmin.width.max(fmin.width), cmin.height.max(fmin.height));
    let max = (cmax.width.min(fmax.width), cmax.height.min(fmax.height));
    let [x, y, w, h] = resized_frame(
        drag.start,
        drag.edge,
        m.x - drag.mouse.0,
        m.y - drag.mouse.1,
        min,
        max,
    );
    ns.setFrame_display(NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)), true);
}

/// OS に大きさ変えを渡す (macOS 以外)。
#[cfg(not(target_os = "macos"))]
pub(crate) fn start_os_resize(window: &winit::window::Window, edge: ResizeEdge) {
    use winit::window::ResizeDirection as D;
    let dir = match edge {
        ResizeEdge::North => D::North,
        ResizeEdge::South => D::South,
        ResizeEdge::East => D::East,
        ResizeEdge::West => D::West,
        ResizeEdge::NorthEast => D::NorthEast,
        ResizeEdge::NorthWest => D::NorthWest,
        ResizeEdge::SouthEast => D::SouthEast,
        ResizeEdge::SouthWest => D::SouthWest,
    };
    // 未対応の環境 (web 等) は Err を返すだけ。押下は引き取ったので何も起きない。
    let _ = window.drag_resize_window(dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: ResizeHandles = ResizeHandles {
        edge: 4.0,
        corner: 16.0,
    };

    #[test]
    fn bottom_corners_are_wider_than_the_edges() {
        let (w, h) = (800.0, 600.0);
        assert_eq!(edge_at(H, 790.0, 590.0, w, h), Some(ResizeEdge::SouthEast), "下の角は 16px の四角");
        assert_eq!(edge_at(H, 10.0, 590.0, w, h), Some(ResizeEdge::SouthWest));
        assert_eq!(edge_at(H, 790.0, 10.0, w, h), None, "上の角は広げない (帯の部品と重なる)");
        assert_eq!(edge_at(H, 798.0, 2.0, w, h), Some(ResizeEdge::NorthEast), "上の角も縁の帯の幅では効く");
        assert_eq!(edge_at(H, 2.0, 2.0, w, h), Some(ResizeEdge::NorthWest));
    }

    #[test]
    fn edges_are_a_thin_band() {
        let (w, h) = (800.0, 600.0);
        assert_eq!(edge_at(H, 797.0, 300.0, w, h), Some(ResizeEdge::East));
        assert_eq!(edge_at(H, 795.0, 300.0, w, h), None, "帯の外");
        assert_eq!(edge_at(H, 1.0, 300.0, w, h), Some(ResizeEdge::West));
        assert_eq!(edge_at(H, 400.0, 1.0, w, h), Some(ResizeEdge::North));
        assert_eq!(edge_at(H, 400.0, 598.0, w, h), Some(ResizeEdge::South));
        assert_eq!(edge_at(H, 400.0, 300.0, w, h), None);
        assert_eq!(edge_at(H, 801.0, 300.0, w, h), None, "窓の外");
    }

    #[test]
    fn dragging_the_bottom_right_corner_keeps_the_top_left() {
        // AppKit の座標: 下へ引くと dy は負
        let f = resized_frame([100.0, 200.0, 800.0, 600.0], ResizeEdge::SouthEast, 50.0, -30.0, (100.0, 100.0), (1e9, 1e9));
        assert_eq!(f, [100.0, 170.0, 850.0, 630.0]);
        // 上の辺 (y + h) は動かない
        assert_eq!(f[1] + f[3], 800.0);
    }

    #[test]
    fn dragging_the_left_and_top_edges_moves_the_origin_only_as_needed() {
        let start = [100.0, 200.0, 800.0, 600.0];
        // 左の辺を左へ 40: 右の辺 (900) は留まる
        let f = resized_frame(start, ResizeEdge::West, -40.0, 0.0, (100.0, 100.0), (1e9, 1e9));
        assert_eq!(f, [60.0, 200.0, 840.0, 600.0]);
        // 上の辺を上へ 20 (AppKit では dy 正): 下の辺 (y) は留まる
        let f = resized_frame(start, ResizeEdge::North, 0.0, 20.0, (100.0, 100.0), (1e9, 1e9));
        assert_eq!(f, [100.0, 200.0, 800.0, 620.0]);
    }

    #[test]
    fn stops_at_the_minimum_without_sliding_the_far_edges() {
        let start = [100.0, 200.0, 800.0, 600.0];
        // 左上の角を右下へ大きく押し込む: 最小で止まり、右の辺と下の辺は留まる
        let f = resized_frame(start, ResizeEdge::NorthWest, 2000.0, -2000.0, (560.0, 360.0), (1e9, 1e9));
        assert_eq!(f, [340.0, 200.0, 560.0, 360.0]);
        assert_eq!(f[0] + f[2], 900.0);
        let f = resized_frame(start, ResizeEdge::SouthEast, 5000.0, -5000.0, (100.0, 100.0), (1000.0, 700.0));
        assert_eq!(f, [100.0, 100.0, 1000.0, 700.0], "最大でも止まる");
    }

    #[test]
    fn every_edge_has_a_resize_cursor() {
        for e in [
            ResizeEdge::North,
            ResizeEdge::South,
            ResizeEdge::East,
            ResizeEdge::West,
            ResizeEdge::NorthEast,
            ResizeEdge::NorthWest,
            ResizeEdge::SouthEast,
            ResizeEdge::SouthWest,
        ] {
            assert_ne!(e.cursor(), Cursor::Default, "{e:?}");
        }
    }
}
