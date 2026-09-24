//! surface を何 px で張るか ([#89](https://github.com/Mutafika/sabitori/issues/89))。
//!
//! レイアウト幅は `surface の幅 / scale_factor` で出している。native では
//! winit の `Resized` がデバイス px を届けるので、そのまま surface に渡せば
//! 辻褄が合う。
//!
//! **web では `Resized` の値を信用できない。** winit は canvas の大きさを
//! `ResizeObserver` の `devicePixelContentBoxSize` から取るが、Chrome は
//! デバイスのエミュレーション (DevTools / Playwright の `deviceScaleFactor`) で
//! これを **CSS px のまま** 返す。`scale_factor` の方は `devicePixelRatio` (2) に
//! なるので、`1024 / 2 = 512` がレイアウト幅になり、画面の半分の幅で組まれる。
//! canvas の backing store も CSS サイズのままなのでぼやける。
//!
//! そこで web では **CSS の大きさ × `devicePixelRatio`** を自分で出して surface に
//! 渡す。`scale_factor` と同じ `devicePixelRatio` から作るので、割り戻した
//! レイアウト幅は必ず CSS 幅に一致する。`devicePixelContentBoxSize` が正しい
//! 環境でも値は (丸めを除いて) 同じになる。

use winit::dpi::PhysicalSize;
use winit::window::Window;

/// CSS の大きさと `devicePixelRatio` から surface のデバイス px を出す。
///
/// canvas がまだ DOM に無い / `display: none` で CSS の大きさが 0 のときは
/// `None` (呼び側は winit の値へ譲る)。
pub fn physical_from_css(css_w: f64, css_h: f64, dpr: f64) -> Option<PhysicalSize<u32>> {
    if !(css_w > 0.0 && css_h > 0.0 && dpr > 0.0) {
        return None;
    }
    Some(PhysicalSize::new(
        (css_w * dpr).round() as u32,
        (css_h * dpr).round() as u32,
    ))
}

/// 窓の surface を張る大きさ。`reported` は winit が `Resized` などで届けた値。
///
/// native では `reported` をそのまま返す。web では canvas の CSS の大きさ ×
/// `devicePixelRatio` を返す (モジュールの説明を参照)。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn surface_size(window: &Window, reported: PhysicalSize<u32>) -> PhysicalSize<u32> {
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;
        if let Some(canvas) = window.canvas() {
            let rect = canvas.get_bounding_client_rect();
            if let Some(size) =
                physical_from_css(rect.width(), rect.height(), window.scale_factor())
            {
                return size;
            }
        }
    }
    reported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpr_2_doubles_the_css_size() {
        // #89 の再現条件。レイアウト幅 = 2048 / 2 = 1024 で CSS 幅と一致する。
        assert_eq!(
            physical_from_css(1024.0, 768.0, 2.0),
            Some(PhysicalSize::new(2048, 1536))
        );
    }

    #[test]
    fn dpr_1_keeps_the_css_size() {
        assert_eq!(
            physical_from_css(1024.0, 768.0, 1.0),
            Some(PhysicalSize::new(1024, 768))
        );
    }

    #[test]
    fn fractional_sizes_round_to_the_nearest_pixel() {
        // ブラウザのズーム (dpr 1.25) や小数の CSS 幅。切り捨てると 1px 欠ける。
        assert_eq!(
            physical_from_css(1007.6, 600.0, 1.25),
            Some(PhysicalSize::new(1260, 750))
        );
    }

    #[test]
    fn nothing_laid_out_yet_defers_to_winit() {
        assert_eq!(physical_from_css(0.0, 768.0, 2.0), None);
        assert_eq!(physical_from_css(1024.0, 0.0, 2.0), None);
        assert_eq!(physical_from_css(1024.0, 768.0, 0.0), None);
        assert_eq!(physical_from_css(f64::NAN, 768.0, 2.0), None);
    }
}
