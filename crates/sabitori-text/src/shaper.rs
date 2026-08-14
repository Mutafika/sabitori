//! GPU-free text shaping and measurement.
//!
//! Everything here runs on a plain `FontSystem` — no `wgpu::Device`, no
//! surface, no adapter. That split exists because measurement and rendering
//! have different hosts: a GUI needs both, but a headless tool (DXF import,
//! paper layout, PDF export) needs only the numbers, and
//! [`crate::TextRenderer::new`] demands a device it has no way to produce.
//!
//! Keeping the font stack here also means there is **one** place that decides
//! which face a string resolves to. Hosts that measured against their own
//! `FontSystem` had to hand-copy the locale normalization and preferred
//! families, and a copy that drifts silently changes which glyphs get picked.

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};
use sabitori_core::element::TextAlign;
use sabitori_core::{TextMetrics, Typography};

/// Grid (logical px) to which font sizes are snapped before shaping/caching.
///
/// Both the shaping cache and the glyph atlas key on the exact `font_size`
/// bits. A size that varies continuously — e.g. a UI billboard whose size
/// tracks 3D-camera distance, or any size-animated label — therefore misses
/// *both* caches on every frame, forcing a full cosmic-text reshape +
/// per-glyph re-rasterization + an atlas re-upload each frame, and steadily
/// exhausting the non-evicting atlas.
///
/// Snapping to a fine grid lets a slowly-varying size settle onto one stable
/// value that hits the cache, at a sub-pixel error (≤ half a quantum) below the
/// perceptual threshold. Grid-aligned sizes (e.g. integers) are unaffected.
pub const FONT_SIZE_QUANTUM: f32 = 0.25;

/// Snap a logical font size to [`FONT_SIZE_QUANTUM`]. Applied at every text
/// entry point (shape, hit-test, measure) so the cache key, the shaped
/// `Buffer`, the cosmic-text physical/atlas key, and the measured box all agree
/// on one value.
pub(crate) fn quantize_font_size(px: f32) -> f32 {
    if !(px > 0.0) {
        return px;
    } // leave 0 / negative / NaN untouched
    (px / FONT_SIZE_QUANTUM).round() * FONT_SIZE_QUANTUM
}

/// Collapse a region-qualified locale ("ja-JP", "ko_KR") to the bare language
/// tag cosmic-text's han-unification table matches exactly. "zh" keeps its
/// region — cosmic-text distinguishes "zh-HK" / "zh-TW" from the "zh-CN"
/// (Simplified) default, so stripping it would change which face is picked.
pub(crate) fn normalize_han_locale(locale: String) -> String {
    if locale.starts_with("zh") {
        return locale;
    }
    match locale.split(['-', '_']).next() {
        Some(lang) if !lang.is_empty() => lang.to_string(),
        _ => locale,
    }
}

/// Resolve the cosmic-text family for a text run. A per-run named
/// `family_override` (from `ElementStyle::font_family`) wins outright — it
/// exists so one element can opt out of the app-wide families (e.g. a font
/// picker previewing each row in its own face). Otherwise the monospace /
/// sans-serif generic applies, redirected by the preferred families when set.
///
/// A free function over the two preferred fields (not a method): callers hold
/// the result while mutably borrowing `font_system`, so the borrow must stay
/// disjoint from the rest of the shaper.
pub(crate) fn resolve_family<'a>(
    preferred: &'a Option<String>,
    preferred_mono: &'a Option<String>,
    monospace: bool,
    family_override: Option<&'a str>,
) -> cosmic_text::Family<'a> {
    if let Some(name) = family_override {
        return cosmic_text::Family::Name(name);
    }
    if monospace {
        if let Some(name) = preferred_mono {
            cosmic_text::Family::Name(name)
        } else {
            cosmic_text::Family::Monospace
        }
    } else if let Some(name) = preferred {
        cosmic_text::Family::Name(name)
    } else {
        cosmic_text::Family::SansSerif
    }
}

/// Apply [`TextAlign`] to every line of a shaped-to-be buffer.
///
/// **`set_text` の後、 `shape_until_scroll` の前**に呼ぶこと。 `set_text` は
/// `BufferLine` を作り直すので、 先に揃えを入れても消える。 そして
/// `set_align` はレイアウトを捨てるだけなので、 shape 済みの後に呼んでも
/// その回の結果には効かない。
///
/// 揃えは `Buffer` の幅に対して効く。 幅を渡していない (= `f32::MAX`) 呼び
/// 出しでは、 揃える相手の余白が無いので何も起きない — 要素に幅を与えろ、
/// というのがそのまま `text_align` の前提になっている。
pub(crate) fn apply_align(buffer: &mut cosmic_text::Buffer, align: TextAlign) {
    let align = match align {
        // `None` は cosmic-text の既定 (書字方向の先頭) に任せる。 `Left` を
        // 明示すると RTL のときに逆側へ寄る。
        TextAlign::Start => None,
        TextAlign::Center => Some(cosmic_text::Align::Center),
        TextAlign::End => Some(cosmic_text::Align::End),
        TextAlign::Justify => Some(cosmic_text::Align::Justified),
    };
    for line in buffer.lines.iter_mut() {
        line.set_align(align);
    }
}

/// The font stack, and everything that can be answered without a GPU.
///
/// [`crate::TextRenderer`] owns one of these and delegates to it, so on-screen
/// text and headless measurement resolve through exactly the same faces,
/// locale and quantization. A host that needs numbers only — DXF import, paper
/// layout, PDF export — can construct this directly.
pub struct TextShaper {
    pub font_system: FontSystem,
    /// Family name used in place of `Family::SansSerif` when shaping
    /// proportional text. Lets apps pick a specific CJK-capable face so
    /// macOS's generic-sans-serif fallback doesn't route kanji through a
    /// Chinese-styled system font.
    pub preferred_family: Option<String>,
    /// Family name used in place of the generic `Family::Monospace` when
    /// shaping monospace text. Lets apps offer a font picker (e.g. a terminal
    /// choosing Hack / JetBrains Mono over the OS default monospace). `None`
    /// keeps cosmic-text's generic monospace resolution.
    pub preferred_monospace_family: Option<String>,
}

impl TextShaper {
    /// Build a shaper over the system fonts, honouring `SABITORI_LOCALE`
    /// (default `"ja"`).
    ///
    /// The locale matters more than it looks. cosmic-text's han-unification
    /// table matches the locale string EXACTLY ("ja", "ko", "zh-HK", "zh-TW"),
    /// so a region-qualified tag like "ja-JP" falls through to its default arm,
    /// PingFang SC (Simplified Chinese). That splits Japanese across two faces:
    /// kanji (`Script::Han`, keyed on this locale) → PingFang SC, while kana
    /// (hard-coded to "ja") → Hiragino Sans. [`normalize_han_locale`] collapses
    /// the tag so kanji and kana resolve through the same Japanese family.
    ///
    /// Doing it here, rather than in the renderer, is the point: a host that
    /// re-implemented this normalization by hand and got it wrong would measure
    /// against a different face than the screen draws with.
    pub fn new() -> Self {
        let locale = std::env::var("SABITORI_LOCALE").unwrap_or_else(|_| "ja".to_string());
        Self::with_locale(&locale)
    }

    /// Like [`TextShaper::new`] but with an explicit locale. The tag is
    /// normalized the same way, so passing "ja-JP" is safe.
    pub fn with_locale(locale: &str) -> Self {
        let locale = normalize_han_locale(locale.to_string());
        #[cfg(not(target_arch = "wasm32"))]
        let font_system = {
            let mut db = cosmic_text::fontdb::Database::new();
            db.load_system_fonts();
            FontSystem::new_with_locale_and_db(locale, db)
        };
        #[cfg(target_arch = "wasm32")]
        let font_system =
            FontSystem::new_with_locale_and_db(locale, cosmic_text::fontdb::Database::new());
        Self {
            font_system,
            preferred_family: None,
            preferred_monospace_family: None,
        }
    }

    /// Load a font from raw TTF/OTF data. Can be called multiple times to
    /// register additional fonts (e.g. Regular + Bold weights).
    ///
    /// Callers holding shaped results keyed on the old font set must drop them
    /// — [`crate::TextRenderer::load_font`] does this for its caches.
    pub fn load_font(&mut self, data: Vec<u8>) {
        self.font_system.db_mut().load_font_data(data);
    }

    /// 渡した user fonts を system fonts より先に DB に入れ直す。
    ///
    /// cosmic_text の script フォールバックは fontdb の挿入順で最初にグリフを
    /// 持つフォントを採用するため、user font を先に積むと macOS の Hiragino 等
    /// のシステム JP フォントよりバンドル済みの Noto などが優先される。
    pub fn prefer_user_fonts(&mut self, user_fonts: &[Vec<u8>]) {
        let locale = self.font_system.locale().to_string();
        let mut db = cosmic_text::fontdb::Database::new();
        for data in user_fonts {
            db.load_font_data(data.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        db.load_system_fonts();
        self.font_system = FontSystem::new_with_locale_and_db(locale, db);
    }

    /// Set the proportional family. Returns whether it actually changed, so the
    /// caller can drop caches keyed on the old face.
    ///
    /// Reporting the change rather than clearing anything keeps this type free
    /// of the renderer's caches — see [`crate::TextRenderer::set_preferred_family`].
    pub fn set_preferred_family(&mut self, family: Option<String>) -> bool {
        if self.preferred_family != family {
            self.preferred_family = family;
            true
        } else {
            false
        }
    }

    /// Set the monospace family. Returns whether it actually changed.
    pub fn set_preferred_monospace_family(&mut self, family: Option<String>) -> bool {
        if self.preferred_monospace_family != family {
            self.preferred_monospace_family = family;
            true
        } else {
            false
        }
    }

    /// Measure `text` without touching the GPU.
    ///
    /// Returns the laid-out box plus the first line's baseline. `max_width`
    /// constrains wrapping so the height reflects the real line count;
    /// `max_lines` caps the reported height the same way the render path
    /// truncates.
    ///
    /// The baseline comes from cosmic-text's `LayoutRun::line_y`, the same
    /// value the render path uses as the glyph pen origin — so a caller that
    /// places text by baseline lands exactly where sabitori would draw it.
    #[allow(clippy::too_many_arguments)]
    pub fn measure_text(
        &mut self,
        text: &str,
        font_size: f32,
        bold: bool,
        monospace: bool,
        family_override: Option<&str>,
        max_width: Option<f32>,
        max_lines: Option<u32>,
        typo: Typography,
    ) -> TextMetrics {
        // Snap so the measured box matches the (quantized) shaped result exactly.
        let font_size = quantize_font_size(font_size);
        let line_height = typo.line_height_px(font_size);
        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        let shaping_width = max_width.unwrap_or(f32::MAX);
        buffer.set_size(&mut self.font_system, Some(shaping_width), None);

        let family = resolve_family(
            &self.preferred_family,
            &self.preferred_monospace_family,
            monospace,
            family_override,
        );
        let weight = cosmic_text::Weight(typo.resolved_weight(bold));
        let mut attrs = Attrs::new().family(family).weight(weight);
        if typo.italic {
            attrs = attrs.style(cosmic_text::Style::Italic);
        }

        buffer.set_text(&mut self.font_system, text, attrs, Shaping::Advanced);
        apply_align(&mut buffer, typo.align);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut width: f32 = 0.0;
        let mut line_count: usize = 0;
        let mut baseline: Option<f32> = None;
        for run in buffer.layout_runs() {
            // Letter-spacing widens each line by (glyphs-1)*spacing; cosmic-text
            // itself has no letter-spacing, so it's folded in here to keep the
            // measured box in sync with the rendered advance.
            let extra = (run.glyphs.len().saturating_sub(1)) as f32 * typo.letter_spacing;
            width = width.max(run.line_w + extra.max(0.0));
            // First run only: later lines sit `line_height` apart, so one value
            // describes them all.
            baseline.get_or_insert(run.line_y);
            line_count += 1;
        }
        let mut lines = line_count.max(1);
        if let Some(cap) = max_lines {
            lines = lines.min(cap as usize);
        }
        let lines_f = lines as f32;
        // Empty strings can shape to zero runs. Nothing is drawn, so the exact
        // value is moot — report the em box top, which matches the CAD "top is
        // 1em above the baseline" convention callers use this for.
        let baseline = baseline.unwrap_or(font_size);
        // Pad each line by 2px to avoid cosmic-text sub-pixel truncation.
        TextMetrics::new(
            width.ceil() + 2.0,
            (lines_f * line_height).ceil(),
            baseline,
        )
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EM: f32 = 20.0;

    fn shaper() -> TextShaper {
        TextShaper::new()
    }

    fn measure(s: &mut TextShaper, text: &str, max_width: Option<f32>) -> TextMetrics {
        s.measure_text(text, EM, false, false, None, max_width, None, Typography::default())
    }

    /// The headless requirement itself: from construction to a measured box
    /// with no `wgpu::Device` anywhere. If this needed an adapter, none of the
    /// tests below could exist — and neither could a DXF importer or a PDF
    /// writer, which is what forced this split.
    #[test]
    fn measures_without_a_gpu_device() {
        let mut s = shaper();
        let m = measure(&mut s, "室名 R-101", None);
        assert!(m.size.width > 0.0, "{m:?}");
        assert!(m.size.height > 0.0, "{m:?}");
        assert!(m.baseline > 0.0, "{m:?}");
    }

    /// The baseline must land inside the line box, not at its top or past its
    /// bottom. A zero here means the field was never populated.
    #[test]
    fn baseline_sits_inside_the_line_box() {
        let mut s = shaper();
        let m = measure(&mut s, "Baseline", None);
        assert!(
            m.baseline > 0.5 * EM && m.baseline < m.size.height,
            "baseline {} outside (0.5em={}, height={})",
            m.baseline,
            0.5 * EM,
            m.size.height
        );
    }

    /// The baseline describes the FIRST line, so wrapping must not move it —
    /// only the height grows. A host placing a multi-line annotation by its
    /// first baseline would otherwise drift as the text rewraps.
    #[test]
    fn baseline_is_independent_of_wrapping() {
        let long = "This annotation is long enough to wrap when the width shrinks";
        let mut s = shaper();
        let wide = measure(&mut s, long, Some(10_000.0));
        let narrow = measure(&mut s, long, Some(120.0));

        assert!(
            narrow.size.height > wide.size.height,
            "the narrow measurement did not wrap ({} vs {})",
            narrow.size.height,
            wide.size.height
        );
        assert!(
            (narrow.baseline - wide.baseline).abs() < 1e-3,
            "first baseline moved on rewrap: {} vs {}",
            wide.baseline,
            narrow.baseline
        );
    }

    /// Extra leading is split above and below the glyphs, so growing
    /// `line_height` pushes the baseline down by half the added space. This is
    /// exactly the discrepancy that made CAD annotations drift: the box grew
    /// but callers assumed the baseline stayed at 1em.
    #[test]
    fn baseline_follows_half_the_added_leading() {
        let mut s = shaper();
        let mut tight = Typography::default();
        tight.line_height = Some(1.0);
        let mut loose = Typography::default();
        loose.line_height = Some(1.4);

        let a = s.measure_text("Ag", EM, false, false, None, None, None, tight);
        let b = s.measure_text("Ag", EM, false, false, None, None, None, loose);

        let added = b.size.height - a.size.height;
        let moved = b.baseline - a.baseline;
        assert!(added > 0.0, "line_height 1.4 must be taller than 1.0");
        assert!(
            (moved - added / 2.0).abs() < 0.75,
            "baseline moved {moved} for {added} added leading — expected about half"
        );
    }

    /// The locale is normalized in the constructor, not left to the caller.
    /// A host that re-implemented this and forgot would resolve kanji through
    /// PingFang SC (Simplified Chinese) while kana stayed Hiragino — the same
    /// string rendered in two faces.
    #[test]
    fn constructor_normalizes_the_locale() {
        let s = TextShaper::with_locale("ja-JP");
        assert_eq!(s.font_system.locale(), "ja");

        // zh keeps its region: cosmic-text distinguishes these.
        let s = TextShaper::with_locale("zh-TW");
        assert_eq!(s.font_system.locale(), "zh-TW");
    }

    /// Half-width digits do NOT advance half an em. Assuming they do — a common
    /// shortcut — understates the string: a 4-digit dimension budgeted at 2em
    /// but really wider drifts right when centered.
    ///
    /// The exact ratio is face-dependent (measured ~0.56em through the default
    /// sans-serif, ~0.67em through Hiragino Sans), so this pins only the claim
    /// in the name, not a number. That spread is the reason the value has to be
    /// measured through *this* shaper rather than hard-coded by the caller.
    #[test]
    fn half_width_digits_are_wider_than_half_an_em() {
        let mut s = shaper();
        // 4 digits, minus the 2px pad the measurement adds.
        let w = measure(&mut s, "8000", None).size.width - 2.0;
        let per_char = w / 4.0;
        assert!(
            per_char > 0.5 * EM,
            "{per_char} px/char at {EM}px em — the 0.5em shortcut would be safe here, which \
             contradicts the measurements this API exists to expose"
        );
        assert!(per_char < 0.9 * EM, "{per_char} px/char is implausibly wide");
    }

    /// Full-width CJK advances one em. Together with the digit test above this
    /// pins the ratio callers were hand-estimating.
    #[test]
    fn full_width_cjk_advances_about_one_em() {
        let mut s = shaper();
        let w = measure(&mut s, "室名室名", None).size.width - 2.0;
        let per_char = w / 4.0;
        assert!(
            (per_char - EM).abs() < 0.15 * EM,
            "{per_char} px/char at {EM}px em — expected about 1em"
        );
    }


    /// Font sizes snap to the quantum grid so a continuously-drifting size
    /// settles onto one cacheable value; grid-aligned sizes are unchanged, and
    /// the max error stays within half a quantum. 0 / negative pass through.
    #[test]
    fn quantizes_font_size_to_grid() {
        // Grid-aligned (integers are multiples of 0.25) → unchanged.
        for exact in [8.0, 12.0, 14.0, 16.0, 48.0] {
            assert_eq!(quantize_font_size(exact), exact);
        }
        // Snaps to the nearest quantum.
        assert_eq!(quantize_font_size(14.10), 14.0);
        assert_eq!(quantize_font_size(14.20), 14.25);
        assert_eq!(quantize_font_size(14.80), 14.75);
        // Error never exceeds half a quantum, across a fine sweep.
        let mut px = 6.0_f32;
        while px < 60.0 {
            let q = quantize_font_size(px);
            assert!((q - px).abs() <= FONT_SIZE_QUANTUM / 2.0 + 1e-4, "{px} -> {q}");
            px += 0.017;
        }
        // The whole point: a sub-bucket drift maps to ONE stable value (cache
        // hit). Sample mid-bucket centers and jitter within ±(quantum/4).
        for k in 24..240 {
            let center = k as f32 * FONT_SIZE_QUANTUM; // exact grid point
            let v = quantize_font_size(center);
            for d in [-0.06_f32, -0.02, 0.0, 0.03, 0.06] {
                assert_eq!(quantize_font_size(center + d), v, "drift at {center}+{d}");
            }
        }
        // Degenerate inputs pass through untouched.
        assert_eq!(quantize_font_size(0.0), 0.0);
        assert_eq!(quantize_font_size(-3.0), -3.0);
    }

    /// cosmic-text's han-unification table matches "ja" / "ko" / "zh-HK" /
    /// "zh-TW" EXACTLY; anything else falls through to PingFang SC (Simplified
    /// Chinese). Region-qualified tags must collapse to the bare language —
    /// except "zh", whose region picks the Han variant.
    #[test]
    fn collapses_region_except_zh() {
        assert_eq!(normalize_han_locale("ja-JP".into()), "ja");
        assert_eq!(normalize_han_locale("ja".into()), "ja");
        assert_eq!(normalize_han_locale("ko_KR".into()), "ko");
        assert_eq!(normalize_han_locale("en-US".into()), "en");
        assert_eq!(normalize_han_locale("zh-HK".into()), "zh-HK");
        assert_eq!(normalize_han_locale("zh-TW".into()), "zh-TW");
        assert_eq!(normalize_han_locale("zh-CN".into()), "zh-CN");
    }
}
