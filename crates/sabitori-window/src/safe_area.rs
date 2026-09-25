//! 画面のうち**システムの UI に隠れない領域**（セーフエリア）の余白。
//!
//! iOS では窓が画面全体（ステータスバー・Dynamic Island・ホームインジケータの下まで）を
//! 覆うので、アプリは上下の余白を空けないと中身が隠れる。余白の量は機種と向きで違う
//! （Dynamic Island 59pt / ノッチ 47pt / ホームボタン機 20pt / 横向きは左右に出る）ので、
//! 定数で決め打ちすると別の機種でずれる。
//!
//! winit は iOS で `inner_*` をセーフエリア、`outer_*` を窓全体として返すので、その差から出す。
//! iOS 以外は 0（デスクトップの窓・ブラウザにはセーフエリアの概念が無い）。
use sabitori_core::Edges;
use winit::window::Window;

/// 窓の四辺のセーフエリアの余白（論理 px）。
pub fn safe_area(window: &Window) -> Edges<f32> {
    #[cfg(target_os = "ios")]
    {
        let scale = window.scale_factor();
        let (Ok(outer), Ok(inner)) = (window.outer_position(), window.inner_position()) else {
            return Edges::default();
        };
        let (os, is) = (window.outer_size(), window.inner_size());
        return insets(
            (outer.x as f64, outer.y as f64, os.width as f64, os.height as f64),
            (inner.x as f64, inner.y as f64, is.width as f64, is.height as f64),
            scale,
        );
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = window;
        Edges::default()
    }
}

/// 窓全体とセーフエリアの矩形（物理 px・`(x, y, w, h)`）から四辺の余白（論理 px）を出す。
/// 端数や負値（回転の途中で矩形が食い違う瞬間）は 0 に寄せる。
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn insets(outer: (f64, f64, f64, f64), inner: (f64, f64, f64, f64), scale: f64) -> Edges<f32> {
    let s = if scale > 0.0 { scale } else { 1.0 };
    let top = (inner.1 - outer.1).max(0.0);
    let left = (inner.0 - outer.0).max(0.0);
    let bottom = (outer.1 + outer.3 - (inner.1 + inner.3)).max(0.0);
    let right = (outer.0 + outer.2 - (inner.0 + inner.2)).max(0.0);
    let l = |v: f64| (v / s).round() as f32;
    Edges::new(l(top), l(right), l(bottom), l(left))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// iPhone 17 Pro Max 縦（440×956pt @3x）: 上 62pt・下 34pt。
    #[test]
    fn portrait_phone_has_top_and_bottom_insets() {
        let e = insets((0.0, 0.0, 1320.0, 2868.0), (0.0, 186.0, 1320.0, 2580.0), 3.0);
        assert_eq!(e, Edges::new(62.0, 0.0, 34.0, 0.0));
    }

    /// 横向きは左右（と下）に出て、上は 0。
    #[test]
    fn landscape_phone_has_side_insets() {
        let e = insets((0.0, 0.0, 2868.0, 1320.0), (186.0, 0.0, 2496.0, 1257.0), 3.0);
        assert_eq!(e, Edges::new(0.0, 62.0, 21.0, 62.0));
    }

    /// セーフエリアが窓と同じ（デスクトップ相当）なら全部 0。
    #[test]
    fn no_insets_when_inner_equals_outer() {
        let r = (0.0, 0.0, 800.0, 600.0);
        assert_eq!(insets(r, r, 2.0), Edges::default());
    }
}
