use serde::{Deserialize, Serialize};

/// Linear RGBA color. All components are in `[0.0, 1.0]`.
///
/// # This is linear, not sRGB
///
/// The surface is sRGB (see `sabitori-gpu`), so the hardware applies the
/// linear→sRGB encode on write. Everything handed to the GPU must therefore be
/// **linear**, and that is what this type stores.
///
/// The catch: a mid-grey is `0.216`, not `0.5`. Pick colors the way a design
/// tool shows them and let the conversion happen here:
///
/// ```
/// # use sabitori_core::Color;
/// let ink = Color::from_hex("#16161B");    // sRGB → converted
/// let same = Color::srgb(0.086, 0.086, 0.106, 1.0); // sRGB floats → converted
/// let raw = Color::linear(0.008, 0.008, 0.011, 1.0); // already linear → stored as-is
/// ```
///
/// [`Color::new`] and [`Color::linear`] do **not** convert. Passing an sRGB
/// value to them makes the color come out noticeably light — `Color::new(0.5,
/// 0.5, 0.5, 1.0)` renders as `#BCBCBC`, not `#808080`.
///
/// # Alpha is not premultiplied
///
/// Rect and ring colors reach the GPU un-premultiplied; the shaders apply alpha
/// during SDF coverage. Only the glyph pipeline blends with premultiplied alpha,
/// and it premultiplies internally.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);

    /// Build from **linear** components. No conversion is applied.
    ///
    /// If you have an sRGB value (anything a design tool, CSS, or a hex code
    /// gave you), use [`Color::srgb`] or [`Color::from_hex`] instead — this
    /// constructor will make it come out too light.
    ///
    /// See [`Color::linear`] for a name that says so at the call site.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Build from **linear** components — same as [`Color::new`], named so the
    /// intent is visible where it's called.
    ///
    /// The two constructors differ only in name. `Color` cannot tell the two
    /// color spaces apart at the type level, so the name is the only signal a
    /// reader gets.
    pub const fn linear(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::new(r, g, b, a)
    }

    /// Build from **sRGB** float components in `[0.0, 1.0]`, converting to linear.
    ///
    /// The float counterpart of [`Color::from_srgb8`]. Use this when a color
    /// comes from a design tool or a picker rather than a hex string. Alpha is
    /// linear either way — it is not a color channel and is never encoded.
    pub fn srgb(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: srgb_to_linear(r),
            g: srgb_to_linear(g),
            b: srgb_to_linear(b),
            a,
        }
    }

    pub fn from_hex(hex: &str) -> Self {
        let hex = hex.trim_start_matches('#');
        let len = hex.len();

        let (r, g, b, a) = match len {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
                let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
                (r, g, b, 255u8)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
                let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
                let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255);
                (r, g, b, a)
            }
            _ => (0, 0, 0, 255),
        };

        Self::from_srgb8(r, g, b, a)
    }

    /// Convert sRGB 8-bit values to linear float.
    pub fn from_srgb8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: srgb_to_linear(r as f32 / 255.0),
            g: srgb_to_linear(g as f32 / 255.0),
            b: srgb_to_linear(b as f32 / 255.0),
            a: a as f32 / 255.0,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Convert back to sRGB 8-bit values (inverse of [`Color::from_srgb8`]).
    /// ユーザー向け表示（RGB 0–255、 hex 文字列等）はこちらの値を使う。
    pub fn to_srgb8(self) -> (u8, u8, u8, u8) {
        let to8 = |c: f32| (linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u8;
        (
            to8(self.r),
            to8(self.g),
            to8(self.b),
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        )
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Linearly interpolate between two colors.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn lighten(self, amount: f32) -> Self {
        self.lerp(Self::WHITE, amount)
    }

    pub fn darken(self, amount: f32) -> Self {
        self.lerp(Self::BLACK, amount)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb8_round_trip() {
        for (r, g, b, a) in [(0, 0, 0, 255), (255, 255, 255, 255), (128, 128, 144, 128), (255, 128, 0, 0)] {
            let c = Color::from_srgb8(r, g, b, a);
            assert_eq!(c.to_srgb8(), (r, g, b, a));
        }
    }

    /// `new` / `linear` は変換しない。ここが変換を始めると、既存の呼び出し元が
    /// 全部暗くなる（linear を linear として渡しているため）。
    #[test]
    fn new_and_linear_store_components_verbatim() {
        let c = Color::new(0.5, 0.25, 0.125, 0.75);
        assert_eq!((c.r, c.g, c.b, c.a), (0.5, 0.25, 0.125, 0.75));
        assert_eq!(Color::linear(0.5, 0.25, 0.125, 0.75), c);
    }

    /// `srgb` は変換する。`new` と同じ引数で別の色になるのが正しい。
    /// この差こそが #36 の中身なので、消えたら気づけるようにしておく。
    #[test]
    fn srgb_converts_and_new_does_not() {
        let converted = Color::srgb(0.5, 0.5, 0.5, 1.0);
        let verbatim = Color::new(0.5, 0.5, 0.5, 1.0);
        assert_ne!(converted, verbatim, "srgb と new が同じ＝変換していない");
        // sRGB 0.5 は linear 0.214。中間グレーは 0.5 ではない。
        assert!((converted.r - 0.2140).abs() < 1e-3, "got {}", converted.r);
    }

    /// 同じ色を別の入口から入れたら同じ所に着く。
    /// from_hex(#808080) と srgb(0.502..) が割れたら、どちらかの経路が壊れている。
    #[test]
    fn srgb_matches_from_hex_for_the_same_color() {
        let a = Color::from_hex("#808080");
        let b = Color::srgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0);
        for (x, y) in [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)] {
            assert!((x - y).abs() < 1e-6, "from_hex と srgb がずれている: {x} vs {y}");
        }
    }

    /// アルファは色ではないので、どの入口でも素通しする。
    #[test]
    fn alpha_is_never_encoded() {
        assert_eq!(Color::srgb(0.0, 0.0, 0.0, 0.5).a, 0.5);
        assert_eq!(Color::new(0.0, 0.0, 0.0, 0.5).a, 0.5);
    }
}
