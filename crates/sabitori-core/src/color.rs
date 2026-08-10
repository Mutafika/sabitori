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

    // -----------------------------------------------------------------
    // Contrast (WCAG 2.x)
    // -----------------------------------------------------------------

    /// WCAG 2.x の相対輝度 `[0.0, 1.0]`。
    ///
    /// 定義は「**線形化した** sRGB 成分の `0.2126R + 0.7152G + 0.0722B`」で、
    /// [`Color`] は既に線形で持っているので**ガンマ戻しは要らない**。hex 文字列
    /// から手で計算するコードが `((c + 0.055) / 1.055).powf(2.4)` を挟むのは、
    /// 出発点が sRGB 符号化値だから。ここで同じことをすると二重に戻すことになり、
    /// 暗い色ほど大きく外れる。
    ///
    /// アルファは見ない。半透明の色の輝度は地と合成するまで決まらないので、
    /// 先に [`Color::over`] で潰すこと。
    ///
    /// 成分は `[0, 1]` に丸めてから計算する。バネ補間は行き過ぎる（`Animated` は
    /// overshoot する）ので、アニメーション中の色は一時的に範囲外の成分を持ちうる。
    /// 画面に出るのは丸めた後の色（[`Color::to_srgb8`] と同じ）なので、輝度もそちらに
    /// 合わせる — でないと比が 21 を超えて、返り値の範囲が doc と食い違う。
    pub fn luminance(self) -> f32 {
        0.2126 * self.r.clamp(0.0, 1.0)
            + 0.7152 * self.g.clamp(0.0, 1.0)
            + 0.0722 * self.b.clamp(0.0, 1.0)
    }

    /// 2 色のコントラスト比 `[1.0, 21.0]`。WCAG 2.x の
    /// `(L_明 + 0.05) / (L_暗 + 0.05)`。
    ///
    /// 目安: 本文 4.5、大きい文字と UI 部品 3.0（WCAG AA）。
    ///
    /// **両方が不透明であること**を前提にする。半透明が絡むなら先に
    /// [`Color::over`] で地に合成する — でないと画面に出ている値ではなくなる。
    pub fn contrast_ratio(self, other: Self) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// `self` を `bg` の上に重ねた結果（source-over 合成）。
    ///
    /// 合成は**線形空間**で行う。サーフェスが sRGB なのでハードウェアが読み書きで
    /// デコード/エンコードし、ブレンドは線形値の上で起きる — つまりこの計算は
    /// 実際に画面へ出る色そのものになる。
    ///
    /// これは非自明で、**hex を 0–255 のまま混ぜた手計算とは違う値が出る**。
    /// 黒を α0.5 で白に重ねた場合:
    ///
    /// - sRGB 空間で混ぜる（素朴な手計算）→ `#808080`、白に対して 3.98:1
    /// - 線形空間で混ぜる（実際の描画）→ `#BCBCBC`、白に対して 1.91:1
    ///
    /// アプリの下に別の絵がある（図面ラスタの上に UI を敷く等）場合、コントラストの
    /// 相手は地の色ではなく**合成結果**になる。その時はこれで潰してから
    /// [`Color::contrast_ratio`] に渡す。
    ///
    /// 結果のアルファは `a_s + a_b(1 - a_s)`。`fg.over(mid).over(paper)` と重ねられる。
    pub fn over(self, bg: Self) -> Self {
        let (sa, ba) = (self.a.clamp(0.0, 1.0), bg.a.clamp(0.0, 1.0));
        let out_a = sa + ba * (1.0 - sa);
        if out_a <= 0.0 {
            return Self::TRANSPARENT;
        }
        let mix = |s: f32, b: f32| (s * sa + b * ba * (1.0 - sa)) / out_a;
        Self {
            r: mix(self.r, bg.r),
            g: mix(self.g, bg.g),
            b: mix(self.b, bg.b),
            a: out_a,
        }
    }

    /// `bg` の上で `min_ratio` を満たすまで、白か黒へ**最小限だけ**寄せた色。
    ///
    /// 既に満たしていれば `self` をそのまま返す。寄せる向きは `bg` から見て
    /// コントラストを稼げる側（暗い地なら白へ、明るい地なら黒へ）。色相は
    /// 保つが、白／黒を混ぜるぶん彩度は落ちる。
    ///
    /// `self` が半透明なら、比の計算は `bg` に合成した結果で行い、返り値は元の
    /// アルファを保つ。`bg` は不透明であることを前提にする（半透明なら先に
    /// [`Color::over`] で潰す）。
    ///
    /// **満たせないこともある。** 白でも黒でも届かない地（中間グレー地に対する
    /// 高い比など）では、届く限り寄せた色を返す — 呼び出し側で
    /// [`Color::contrast_ratio`] を見て、地の側を変える判断ができる。
    pub fn readable_on(self, bg: Self, min_ratio: f32) -> Self {
        let ratio_of = |c: Self| c.over(bg).contrast_ratio(bg);
        if ratio_of(self) >= min_ratio {
            return self;
        }
        // 地から遠い方の極へ寄せる。地が暗ければ白、明るければ黒。
        let target = if bg.contrast_ratio(Self::WHITE) >= bg.contrast_ratio(Self::BLACK) {
            Self::WHITE
        } else {
            Self::BLACK
        };
        let toward = |t: f32| {
            let mixed = self.lerp(target, t);
            Self { a: self.a, ..mixed }
        };
        if ratio_of(toward(1.0)) < min_ratio {
            // 極でも届かない。届く限り寄せた色を返す。
            return toward(1.0);
        }
        // 満たす最小の t を二分探索する。比は t に対して単調ではない場合が
        // あるが (色相によっては途中で地の輝度をまたぐ)、極が満たす以上
        // 上端は必ず満たすので、上端側の境界に収束する。
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            if ratio_of(toward(mid)) >= min_ratio {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        toward(hi)
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

    // -----------------------------------------------------------------
    // Contrast (#7)
    // -----------------------------------------------------------------

    /// 白 1.0 / 黒 0.0、その 2 つで 21:1。WCAG の両端。
    #[test]
    fn luminance_and_ratio_hit_the_known_endpoints() {
        assert!((Color::WHITE.luminance() - 1.0).abs() < 1e-6);
        assert!((Color::BLACK.luminance() - 0.0).abs() < 1e-6);
        assert!((Color::WHITE.contrast_ratio(Color::BLACK) - 21.0).abs() < 1e-4);
        // 比は向きに依らない。
        assert_eq!(
            Color::WHITE.contrast_ratio(Color::BLACK),
            Color::BLACK.contrast_ratio(Color::WHITE)
        );
        // 同じ色同士は 1:1。
        let c = Color::from_hex("#7aa2f7");
        assert!((c.contrast_ratio(c) - 1.0).abs() < 1e-6);
    }

    /// 範囲外の成分でも比は `[1, 21]` に収まること。
    ///
    /// バネ補間は行き過ぎるので、アニメーション中の色は一時的に 1.0 を超えた成分を
    /// 持ちうる。丸めずに計算すると比が 31 まで出て、返り値の範囲が doc と食い違う。
    #[test]
    fn out_of_range_components_are_clamped_like_the_screen_does() {
        let hot = Color::new(1.5, 1.5, 1.5, 1.0);
        assert!((hot.luminance() - 1.0).abs() < 1e-6);
        assert!((hot.contrast_ratio(Color::BLACK) - 21.0).abs() < 1e-4);

        let cold = Color::new(-0.3, -0.3, -0.3, 1.0);
        assert!((cold.luminance() - 0.0).abs() < 1e-6);
        assert!((cold.contrast_ratio(Color::WHITE) - 21.0).abs() < 1e-4);
    }

    /// **本題**: `Color` は linear 保持なので、輝度にガンマ戻しを挟んではいけない。
    ///
    /// hex から手で計算するコードは `((c+0.055)/1.055).powf(2.4)` を通すが、
    /// それは出発点が sRGB 符号化値だから。ここで同じことをすると二重に戻る。
    /// `#808080` の正しい相対輝度は 0.2159 で、二重に戻すと 0.0405 まで落ちる。
    #[test]
    fn luminance_does_not_gamma_expand_twice() {
        let grey = Color::from_hex("#808080");
        assert!(
            (grey.luminance() - 0.2159).abs() < 1e-3,
            "got {} — 二重にガンマ戻ししていないか",
            grey.luminance()
        );
        // 白地に対する #808080 は WCAG の教科書値 3.95〜4.0 近辺。
        let r = grey.contrast_ratio(Color::WHITE);
        assert!((3.9..4.1).contains(&r), "got {r}");
    }

    /// 合成は**線形空間**で起きる（sRGB サーフェス + ハードウェアブレンド）。
    /// 黒を α0.5 で白に重ねると `#808080` ではなく `#BCBCBC` になる。
    /// ここが `#808080` に戻ったら、画面に出ている色と計算がずれ始めている。
    #[test]
    fn over_composites_in_linear_space() {
        let out = Color::BLACK.with_alpha(0.5).over(Color::WHITE);
        assert_eq!(out.to_srgb8(), (188, 188, 188, 255));
        let r = out.contrast_ratio(Color::WHITE);
        assert!((1.85..1.95).contains(&r), "got {r}");
    }

    /// 半透明どうしを重ねてもアルファが正しく積み上がる（`fg.over(mid).over(paper)`）。
    #[test]
    fn over_accumulates_alpha() {
        let half = Color::BLACK.with_alpha(0.5);
        assert!((half.over(half).a - 0.75).abs() < 1e-6);
        // 完全不透明の地に重ねたら不透明になる。
        assert_eq!(half.over(Color::WHITE).a, 1.0);
        // 何も無い所に重ねたら元のまま。
        let onto_nothing = half.over(Color::TRANSPARENT);
        assert!((onto_nothing.a - 0.5).abs() < 1e-6);
        // 透明を重ねても地は変わらない。
        assert_eq!(Color::TRANSPARENT.over(Color::WHITE), Color::WHITE);
    }

    /// issue #7 の実例: 薄く敷いた色は、**下の紙まで込みで**合成しないと
    /// コントラストが出ない。UI 地に対する比では出てこない値になる。
    ///
    /// 併せて、**線形合成と sRGB 合成で答えが大きく違う**ことを固定する。
    /// 手計算（hex を 0–255 のまま混ぜる）は sRGB 空間の値になるが、画面に出るのは
    /// 線形合成の方。薄い色では差が小さく（1.27 vs 1.20）、濃い色では桁が変わる
    /// （10.7 vs 5.0）。「濃く敷けば十分に暗くなる」という手計算由来の見積もりは、
    /// 実際の描画では大きく外れる。
    #[test]
    fn translucent_ink_on_white_paper_is_measured_after_compositing() {
        let paper = Color::WHITE;

        // α0.28 で紙に敷く → ほとんど紙のまま = 読めない。
        let faint = Color::from_hex("#7aa2f7").with_alpha(0.28).over(paper);
        let faint_ratio = faint.contrast_ratio(paper);
        assert!(faint_ratio < 1.3, "薄く敷いた色は紙に対してほぼ無い: {faint_ratio}");

        // 暗い地を α0.85 で敷く → 線形合成では ~5:1。本文には足りるが、
        // sRGB 空間の手計算が示す ~10.7:1 の半分以下しかない。
        let veil = Color::from_hex("#1a1b26").with_alpha(0.85).over(paper);
        let veil_ratio = veil.contrast_ratio(paper);
        assert!(
            (4.8..5.3).contains(&veil_ratio),
            "線形合成での実値は ~5:1。10 を超えたら sRGB 空間で混ぜている: {veil_ratio}"
        );
        assert!(veil_ratio > 4.5, "本文コントラスト (AA) は満たすこと");
    }

    /// 足りていれば触らない。触ってしまうと、設計で選んだ色が黙って変わる。
    #[test]
    fn readable_on_leaves_a_passing_color_alone() {
        let bg = Color::from_hex("#1a1b26");
        let fg = Color::WHITE;
        assert_eq!(fg.readable_on(bg, 4.5), fg);
    }

    /// 暗い地では白へ、明るい地では黒へ寄る。どちらも寄せた後は基準を満たす。
    #[test]
    fn readable_on_reaches_the_target_ratio_from_both_sides() {
        for (bg_hex, fg_hex) in [
            ("#1a1b26", "#3b4261"), // 暗い地に暗い字
            ("#ffffff", "#c4a2f7"), // 明るい地に淡い紫
        ] {
            let bg = Color::from_hex(bg_hex);
            let fg = Color::from_hex(fg_hex);
            assert!(fg.contrast_ratio(bg) < 4.5, "前提: 元は足りていないこと");

            let fixed = fg.readable_on(bg, 4.5);
            let got = fixed.contrast_ratio(bg);
            assert!(got >= 4.5, "{bg_hex} の上で {fg_hex} が届いていない: {got}");
            // 最小限しか寄せない = 大きく行き過ぎない。
            assert!(got < 4.5 * 1.6, "寄せ過ぎ ({bg_hex} / {fg_hex}): {got}");
        }
    }

    /// 届かない地では、届く限り寄せた色を返す（黙って諦めた色を返さない）。
    /// 中間グレー地は白でも黒でも 21:1 には届かない。
    #[test]
    fn readable_on_returns_the_best_effort_when_unreachable() {
        let bg = Color::from_hex("#808080");
        let fixed = Color::from_hex("#7f7f7f").readable_on(bg, 21.0);
        let best = bg
            .contrast_ratio(Color::WHITE)
            .max(bg.contrast_ratio(Color::BLACK));
        let got = fixed.contrast_ratio(bg);
        assert!(
            (got - best).abs() < 1e-3,
            "届く限界まで寄せること: got {got}, 限界 {best}"
        );
    }

    /// 半透明の前景は、地に合成した結果で判定し、アルファは保って返す。
    #[test]
    fn readable_on_keeps_alpha_and_judges_the_composited_result() {
        let bg = Color::from_hex("#1a1b26");
        let fg = Color::from_hex("#3b4261").with_alpha(0.6);
        let fixed = fg.readable_on(bg, 4.5);

        assert!((fixed.a - 0.6).abs() < 1e-6, "アルファを保つこと");
        assert!(
            fixed.over(bg).contrast_ratio(bg) >= 4.5,
            "合成後で基準を満たすこと"
        );
    }
}
