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
use sabitori_core::build::{CaretPos, TextShape};
use sabitori_core::{Rect, TextMetrics, Typography};

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

/// wasm32 に埋め込む最終フォールバックフォント。
///
/// | feature | フォント | raw | gzip | 日本語 |
/// |---|---|---|---|---|
/// | `builtin-font-jp` (既定) | HackGen (白源) | 10.2MB | 4.9MB | **出る** |
/// | `builtin-font-latin` | Hack | 302KB | 144KB | 豆腐 |
///
/// 既定が日本語込みなのは、 **Latin だけの組み込みは「日本語を描かない
/// アプリ」に最適化した既定**だから。 sabitori で書く UI はまず日本語を
/// 出すので、 その既定だと結局アプリ側が CJK フォントを積むことになり、
/// 組み込みの 302KB は**一度も字を描かないまま同梱される**。
///
/// HackGen の字形は Hack そのものだが、 **advance は詰めてある**
/// (Hack 0.602em → HackGen 0.527em)。 半角 2 文字が全角 1 文字にちょうど
/// 乗るようにするためで、 TUI の桁が揃うのはこの性質による。 裏を返すと
/// **feature を切り替えると英数字のレイアウトは動く** — 字形が同じでも
/// 幅は同じにならない。
///
/// ## なぜ埋め込むか
///
/// ブラウザにはシステムフォントが無い。 `load_system_fonts()` は wasm では
/// 何も積まないので、 アプリが [`TextShaper::load_font`] で 1 つも渡さないと
/// フォント DB が空のまま最初のシェープに入り、 **cosmic-text の奥で
/// `no default font found` が panic する** (`shape.rs:251` の `expect`)。
///
/// 出てくるのは依存クレート内部のメッセージで、 sabitori の名前も
/// `fonts()` の名前も出ない。 しかも native では system fonts があるので
/// 再現しない ── **wasm に持っていって初めて、 何も描かれない画面と
/// 読めないスタックトレースが出る**。 毎回同じ所で足を取られる。
///
/// なので wasm では常に 1 つ積んでおく。 `default-features = false` で外せる。
///
/// ## `-latin` を選んだ場合に何が残るか
///
/// Hack は Latin + 記号 + 罫線素片で、 **CJK は入っていない**。 日本語 UI を
/// web に出すなら CJK フォントを [`crate::TextRenderer::load_font`] で渡す
/// 必要がある。 ただし失敗の形は変わる ── panic して真っ白ではなく、
/// レイアウトは出て日本語だけが豆腐になるので、 何が足りないか画面で分かる。
///
/// ライセンスは `crates/sabitori-text/assets/LICENSE-{Hack,HackGen}.txt`。
///
/// `test` でも取り込むのは、 wasm に載せる**そのバイト列**が本当にシェープ
/// できることを native のテストから確かめるため。 native の実ビルドには
/// 入らない。
///
/// 両方の feature が立っていれば `-jp` が勝つ ── Cargo の feature は
/// 加算的で排他にできないので、 「広い方を採る」で決めておく。
#[cfg(any(all(feature = "builtin-font-jp", target_arch = "wasm32"), test))]
pub(crate) const BUILTIN_FONT: &[u8] = include_bytes!("../assets/HackGen-Regular.ttf");

#[cfg(all(
    not(feature = "builtin-font-jp"),
    not(test),
    feature = "builtin-font-latin",
    target_arch = "wasm32"
))]
pub(crate) const BUILTIN_FONT: &[u8] = include_bytes!("../assets/Hack-Regular.ttf");

/// `-latin` 側のバイト列。 native のテストから、 こちらも単独でシェープ
/// できることを確かめるために取り込む (実ビルドには入らない)。
#[cfg(test)]
pub(crate) const BUILTIN_FONT_LATIN: &[u8] = include_bytes!("../assets/Hack-Regular.ttf");

/// フォントが 1 つも無いまま呼ばれたときの説明。
///
/// cosmic-text は `expect("no default font found")` で落ちる。 それ自体は
/// 正しい診断だが、 **どこに何を書けば直るのかが書いていない**。 wasm で
/// これを踏むのはほぼ必ず「`fonts()` を実装し忘れた」なので、 そう言う。
const NO_FONTS: &str = "sabitori: フォントが 1 つも読み込まれていない。\n\
     テキストをシェープできないので、 このままでは cosmic-text が\n\
     `no default font found` で落ちる。\n\
     \n\
     wasm では system fonts が使えない。 `DeclarativeApp::fonts()` で\n\
     最低 1 つ TTF/OTF を返すこと:\n\
     \n\
         fn fonts(&self) -> Vec<Vec<u8>> {\n\
             vec![include_bytes!(\"../assets/fonts/YourFont.ttf\").to_vec()]\n\
         }\n\
     \n\
     (既定では wasm に HackGen が自動で積まれる。 これが出ているなら\n\
     `builtin-font-jp` / `builtin-font-latin` の両方が切られているか、\n\
     native で system fonts が 1 つも無い環境。)";

/// [`BUILTIN_FONT`] を DB に積む。 feature が切られている / native の実ビルド
/// では何もしない。
fn load_builtin_font(_db: &mut cosmic_text::fontdb::Database) {
    #[cfg(all(
        any(feature = "builtin-font-jp", feature = "builtin-font-latin"),
        target_arch = "wasm32"
    ))]
    _db.load_font_data(BUILTIN_FONT.to_vec());
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
        let mut db = cosmic_text::fontdb::Database::new();
        #[cfg(not(target_arch = "wasm32"))]
        db.load_system_fonts();
        // wasm には system fonts が無い。 [`BUILTIN_FONT`] を参照。
        load_builtin_font(&mut db);
        Self {
            font_system: FontSystem::new_with_locale_and_db(locale, db),
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
        // user fonts の**後ろ**。 挿入順が優先順なので、 アプリが渡した face
        // が常に勝ち、 組み込みは穴埋めにしか使われない。
        load_builtin_font(&mut db);
        self.font_system = FontSystem::new_with_locale_and_db(locale, db);
    }

    /// システムフォントを**一切使わず**、 渡したフォントだけで組む。
    ///
    /// wasm と同じ条件を native で再現するためのもの。 ブラウザには
    /// システムフォントが無いので、 アプリが `fonts()` で渡した物だけが
    /// 使える ── native で動かしている限り、 その不足はシステムフォントが
    /// 埋めてしまって**気づけない**。
    ///
    /// ```ignore
    /// // 自分のアプリの fonts() が、 自分の UI の文字を全部持っているか
    /// let mut s = TextShaper::with_fonts_only("ja", &my_app.fonts());
    /// assert!(s.missing_glyphs("設定を保存", shape).is_empty());
    /// ```
    pub fn with_fonts_only(locale: &str, fonts: &[Vec<u8>]) -> Self {
        let locale = normalize_han_locale(locale.to_string());
        let mut db = cosmic_text::fontdb::Database::new();
        for data in fonts {
            db.load_font_data(data.clone());
        }
        Self {
            font_system: FontSystem::new_with_locale_and_db(locale, db),
            preferred_family: None,
            preferred_monospace_family: None,
        }
    }

    /// `text` のうち、 いまのフォント構成では**描けない**文字。 重複は畳む。
    ///
    /// 「豆腐 (.notdef) になる文字」を返す。 幅は豆腐でも出るので、 測って
    /// 0 でないことを確かめても**字が出ているかは分からない**。 グリフ ID が
    /// 0 かどうかで見るしかない。
    pub fn missing_glyphs(&mut self, text: &str, shape: TextShape<'_>) -> Vec<char> {
        let buffer = self.shaped(text, shape);
        let starts = logical_line_starts(&buffer);
        let mut out: Vec<char> = Vec::new();
        for run in buffer.layout_runs() {
            let base = starts.get(run.line_i).copied().unwrap_or(0);
            for g in run.glyphs {
                if g.glyph_id != 0 {
                    continue;
                }
                let lo = (base + g.start).min(text.len());
                let hi = (base + g.end).min(text.len());
                let Some(slice) = text.get(lo..hi) else { continue };
                for c in slice.chars() {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
        out
    }

    /// フォントが 1 つでも読み込まれているか。
    ///
    /// `false` のままシェープすると cosmic-text の奥で panic する。 ホストが
    /// 起動時に確かめて、 自前の案内を出したいとき用。
    pub fn has_fonts(&self) -> bool {
        self.font_system.db().len() > 0
    }

    /// シェープに入る手前で止める。 メッセージは [`NO_FONTS`]。
    ///
    /// このまま進むと cosmic-text の `expect` に到達するので、 どちらにせよ
    /// 落ちる。 落ちる場所を**直し方が書いてある方**に寄せているだけ。
    /// 比較 1 回なので毎回呼んでよい。
    fn require_fonts(&self) {
        assert!(self.has_fonts(), "{NO_FONTS}");
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
        self.require_fonts();
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

    /// [`TextShape`] のとおりに整形した `Buffer` を作る。
    ///
    /// 折り返し系の 3 つのクエリが共有する下ごしらえ。 `measure_text` と同じ
    /// 手順を踏むこと — ここがずれると、 測った箱と実際のキャレット位置が
    /// 食い違う。
    fn shaped(&mut self, text: &str, shape: TextShape<'_>) -> Buffer {
        self.require_fonts();
        let font_size = quantize_font_size(shape.font_size);
        let metrics = Metrics::new(font_size, shape.typo.line_height_px(font_size));
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(
            &mut self.font_system,
            Some(shape.wrap_width.unwrap_or(f32::MAX)),
            None,
        );

        let family = resolve_family(
            &self.preferred_family,
            &self.preferred_monospace_family,
            shape.monospace,
            shape.font_family,
        );
        let weight = cosmic_text::Weight(shape.typo.resolved_weight(shape.bold));
        let mut attrs = Attrs::new().family(family).weight(weight);
        if shape.typo.italic {
            attrs = attrs.style(cosmic_text::Style::Italic);
        }

        buffer.set_text(&mut self.font_system, text, attrs, Shaping::Advanced);
        apply_align(&mut buffer, shape.typo.align);
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// `byte_offset` にキャレットを置いたときの位置。
    ///
    /// # 論理行と視覚行
    ///
    /// cosmic-text の `LayoutGlyph::start` は「**論理行の中での**添字」で、
    /// 文字列全体の添字ではない。 1 行しか無いうちは同じ値なので気づかないが、
    /// 改行を入れた瞬間に 2 行目以降が全部 0 起点に戻る。 ここで論理行の開始
    /// 位置を足して絶対値に直している。
    ///
    /// # 境界での寄せ方
    ///
    /// - **折り返しの継ぎ目** (ソフト改行) — 次の視覚行の先頭に置く。 その方が
    ///   「次に打った文字が出る場所」と一致する。
    /// - **改行の直前** (ハード改行) — 前の行の末尾に置く。 `\n` の手前に
    ///   キャレットがあるのだから、 次の行の先頭に飛んではいけない。
    pub fn caret_pos(&mut self, text: &str, byte_offset: usize, shape: TextShape<'_>) -> CaretPos {
        let offset = clamp_to_boundary(text, byte_offset);
        let font_size = quantize_font_size(shape.font_size);
        let line_height = shape.typo.line_height_px(font_size);
        let buffer = self.shaped(text, shape);
        let starts = logical_line_starts(&buffer);

        // 見つからなかったとき用に「offset 以下から始まる最後の行」を覚えておく。
        // ハード改行の直前と空行がここに落ちる。
        let mut fallback: Option<CaretPos> = None;

        for (line, run) in buffer.layout_runs().enumerate() {
            let base = starts.get(run.line_i).copied().unwrap_or(0);
            let (lo, hi) = run_range(&run, base);
            let here = CaretPos { x: 0.0, y: run.line_top, line_height: run.line_height, line };

            if offset >= lo && offset < hi {
                // offset 以降から始まる最初のグリフの左端。
                let x = run
                    .glyphs
                    .iter()
                    .find(|g| base + g.start >= offset)
                    .map(|g| g.x)
                    .unwrap_or_else(|| run.glyphs.last().map_or(0.0, |g| g.x + g.w));
                return CaretPos { x, ..here };
            }
            if offset >= lo {
                let x = run.glyphs.last().map_or(0.0, |g| g.x + g.w);
                fallback = Some(CaretPos { x, ..here });
            }
        }

        fallback.unwrap_or(CaretPos { x: 0.0, y: 0.0, line_height, line: 0 })
    }

    /// テキスト原点からの相対座標に最も近いキャレット位置のバイト添字。
    ///
    /// **範囲外でも必ず答える。** 上に外れたら先頭、 下に外れたら末尾、 行から
    /// 右に外れたらその行の末尾。 `Option` にすると、 欄の余白をクリックした
    /// ときに「何も起きない」になる。
    pub fn offset_at(&mut self, text: &str, point: (f32, f32), shape: TextShape<'_>) -> usize {
        let (px, py) = point;
        let buffer = self.shaped(text, shape);
        let starts = logical_line_starts(&buffer);

        let mut best_run: Option<(f32, cosmic_text::LayoutRun<'_>)> = None;
        for run in buffer.layout_runs() {
            let top = run.line_top;
            let bottom = top + run.line_height;
            // py がこの行の帯に入っていれば即決。 そうでなければ「帯までの
            // 距離」が最小の行を採る (上下に外れた場合の端寄せがこれで済む)。
            let d = if py < top {
                top - py
            } else if py > bottom {
                py - bottom
            } else {
                0.0
            };
            if best_run.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best_run = Some((d, run));
            }
        }

        let Some((_, run)) = best_run else {
            return 0;
        };
        let base = starts.get(run.line_i).copied().unwrap_or(0);
        let (lo, hi) = run_range(&run, base);

        // 行の中で x に最も近い**クラスタ境界**を選ぶ。 グリフの左端と右端の
        // 両方が候補 — 文字の右半分をクリックしたら後ろに置きたい。
        let mut best = lo;
        let mut best_d = f32::MAX;
        for g in run.glyphs {
            for (edge, off) in [(g.x, base + g.start), (g.x + g.w, base + g.end)] {
                let d = (edge - px).abs();
                if d < best_d {
                    best_d = d;
                    best = off;
                }
            }
        }
        if run.glyphs.is_empty() {
            best = lo;
        }
        clamp_to_boundary(text, best.min(hi.max(lo)))
    }

    /// バイト範囲が占める矩形を**視覚行ごとに**返す。
    ///
    /// 折り返しをまたぐ選択が 1 個の矩形で返ると、 行間の余白まで塗って隣の
    /// 行に食い込む。 選択範囲の塗りと IME 変換中の下線がこれを使う。
    pub fn range_rects(
        &mut self,
        text: &str,
        range: (usize, usize),
        shape: TextShape<'_>,
    ) -> Vec<Rect> {
        let lo = clamp_to_boundary(text, range.0.min(range.1));
        let hi = clamp_to_boundary(text, range.0.max(range.1));
        if lo == hi {
            return Vec::new();
        }
        let buffer = self.shaped(text, shape);
        let starts = logical_line_starts(&buffer);

        let mut out = Vec::new();
        for run in buffer.layout_runs() {
            let base = starts.get(run.line_i).copied().unwrap_or(0);
            let mut left = f32::MAX;
            let mut right = f32::MIN;
            for g in run.glyphs {
                // クラスタが選択範囲に少しでも重なれば含める。
                if base + g.end > lo && base + g.start < hi {
                    left = left.min(g.x);
                    right = right.max(g.x + g.w);
                }
            }
            if left <= right {
                out.push(Rect::new(left, run.line_top, right - left, run.line_height));
            }
        }
        out
    }
}

/// `offset` を直前の文字境界まで戻し、 文字列長で頭打ちにする。
fn clamp_to_boundary(text: &str, offset: usize) -> usize {
    let mut n = offset.min(text.len());
    while n > 0 && !text.is_char_boundary(n) {
        n -= 1;
    }
    n
}

/// 各論理行が文字列全体の何バイト目から始まるか。
///
/// cosmic-text は `\n` で論理行に割り、 `BufferLine` のテキストに区切り文字は
/// 含めない。 なので「前の行の長さ + 1」で足していける。 グリフの添字を絶対値に
/// 直すのにこれが要る。
fn logical_line_starts(buffer: &Buffer) -> Vec<usize> {
    let mut starts = Vec::with_capacity(buffer.lines.len());
    let mut acc = 0usize;
    for line in &buffer.lines {
        starts.push(acc);
        acc += line.text().len() + 1; // +1 = cosmic-text が落とした `\n`
    }
    starts
}

/// この視覚行が担当する**絶対**バイト範囲。
///
/// グリフが 1 つも無い視覚行 (改行だけの空行) は `(base, base)` になる。
/// ここを `(0, 0)` に潰すと、 空行のキャレットが全部先頭に飛ぶ。
fn run_range(run: &cosmic_text::LayoutRun<'_>, base: usize) -> (usize, usize) {
    let lo = run.glyphs.iter().map(|g| base + g.start).min();
    let hi = run.glyphs.iter().map(|g| base + g.end).max();
    match (lo, hi) {
        (Some(lo), Some(hi)) => (lo, hi),
        _ => (base, base),
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

/// wasm に積む組み込みフォントと、 フォントが 1 つも無いときの落ち方。
///
/// ここは **native から wasm の挙動を検証している**。 `BUILTIN_FONT` は
/// `cfg(test)` でも取り込まれるので、 「ブラウザに送るまさにそのバイト列」を
/// そのまま組んで確かめられる。 wasm でしか再現しない不具合を wasm に
/// 持っていく前に捕まえるのが目的。
#[cfg(test)]
mod builtin_font_tests {
    use super::*;

    /// 組み込みフォント **だけ** の DB。 wasm の初期状態そのもの
    /// (system fonts が無く、 アプリも何も渡していない)。
    fn wasm_like_shaper() -> TextShaper {
        TextShaper::with_fonts_only("ja", &[BUILTIN_FONT.to_vec()])
    }

    /// フォントが 1 つも無い DB。 `builtin-font` を切った wasm ビルドと、
    /// system fonts が 1 つも無い native 環境がこれ。
    fn empty_shaper() -> TextShaper {
        TextShaper::with_fonts_only("ja", &[])
    }

    fn measure(s: &mut TextShaper, text: &str) -> TextMetrics {
        s.measure_text(text, 16.0, false, false, None, None, None, Typography::default())
    }

    fn shape() -> TextShape<'static> {
        TextShape {
            font_size: 16.0,
            bold: false,
            monospace: false,
            font_family: None,
            wrap_width: None,
            typo: Typography::default(),
        }
    }

    /// 実際に選ばれたグリフ ID。 **0 は .notdef (豆腐)** なので、 「幅が出た」
    /// では見分けられない「字が出ているか」をこれで見る。
    fn glyph_ids(s: &mut TextShaper, text: &str) -> Vec<u16> {
        let buffer = s.shaped(text, shape());
        buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter().map(|g| g.glyph_id))
            .collect()
    }

    /// **これが本題。** wasm に積むバイト列が、 単独で本当にシェープできること。
    ///
    /// 積んであっても壊れていれば結局 `no default font found` に戻るので、
    /// 「ファイルが存在する」ではなく「幅が出る」まで見る。
    #[test]
    fn the_bundled_font_shapes_latin_on_its_own() {
        let mut s = wasm_like_shaper();
        assert!(s.has_fonts());
        let m = measure(&mut s, "Hello, sabitori");
        assert!(m.size.width > 0.0, "幅が出ないなら積めていない: {m:?}");
        assert!(m.size.height > 0.0, "{m:?}");
        assert!(m.baseline > 0.0, "{m:?}");
    }

    /// 罫線素片が出ること。 TUI 風の枠は sabitori の看板なので、 Latin だけ
    /// 出て枠が消える組み合わせは選べない。
    #[test]
    fn the_bundled_font_covers_box_drawing() {
        let mut s = wasm_like_shaper();
        let ids = glyph_ids(&mut s, "┌──┐│└┘");
        assert!(!ids.is_empty(), "グリフが 1 つも出ていない");
        assert!(
            ids.iter().all(|&id| id != 0),
            "豆腐が混ざっている (glyph_id: {ids:?})"
        );
    }

    /// **既定の組み込みは日本語を描ける。** これが `-jp` を既定にした理由
    /// そのもの ── wasm でも `fonts()` を 1 行も書かずに日本語 UI が出る。
    #[test]
    fn the_bundled_font_covers_japanese() {
        let mut s = wasm_like_shaper();
        let missing = s.missing_glyphs("日本語のテキスト、ひらがなとカタカナ。", shape());
        assert!(
            missing.is_empty(),
            "既定の組み込みで日本語が描けない: {missing:?}"
        );
    }

    /// `-latin` を選んだ場合は日本語が豆腐になる。 **ただし panic はしない** ──
    /// レイアウトは出て、 日本語だけが描けない。 「真っ白で読めないスタック
    /// トレース」から「画面を見れば足りない物が分かる」への移動が、 組み込み
    /// フォントの効き目そのもの。
    #[test]
    fn the_latin_variant_lays_out_japanese_without_drawing_it() {
        let mut s = TextShaper::with_fonts_only("ja", &[BUILTIN_FONT_LATIN.to_vec()]);
        let m = measure(&mut s, "日本語のテキスト");
        assert!(m.size.height > 0.0, "落ちずに行の高さは出ること: {m:?}");
        assert!(
            !s.missing_glyphs("日本語", shape()).is_empty(),
            "Hack に CJK は入っていないはず。 差し替えたならこの前提を書き直すこと"
        );
    }

    /// `-latin` 側も単独でシェープできること。 既定から外れた経路は誰も
    /// 通らないまま腐るので、 バイト列が生きているかはここで見る。
    #[test]
    fn the_latin_variant_still_shapes_on_its_own() {
        let mut s = TextShaper::with_fonts_only("ja", &[BUILTIN_FONT_LATIN.to_vec()]);
        let m = measure(&mut s, "Hello, sabitori");
        assert!(m.size.width > 0.0, "{m:?}");
        assert!(s.missing_glyphs("┌──┐ Hello", shape()).is_empty());
    }

    /// **Latin 2 文字 = 全角 1 文字** に乗っていること。
    ///
    /// TUI の見た目はこれが全て ── 罫線と日本語と英数字が同じ桁に揃うのは、
    /// 全角の advance が半角のちょうど 2 倍だから。 Hack + システム日本語
    /// フォントの組み合わせでは**この保証が無い**ので、 枠の中の日本語が
    /// 1 文字ずつずれていく。 HackGen を選んだ理由はここ。
    #[test]
    fn the_bundled_font_puts_latin_and_cjk_on_one_grid() {
        let mut s = wasm_like_shaper();
        let two_latin = measure(&mut s, "AA").size.width;
        let one_cjk = measure(&mut s, "あ").size.width;
        assert!(
            (two_latin - one_cjk).abs() <= 1.0,
            "半角 2 文字 ({two_latin}) と全角 1 文字 ({one_cjk}) が揃っていない"
        );
    }

    /// **組み込みを切り替えると英数字の幅は変わる。**
    ///
    /// HackGen の字形は Hack そのものだが、 advance は詰めてある
    /// (Hack 0.602em → HackGen 0.527em)。 上のグリッドに乗せるためで、
    /// 字形が同じでも**レイアウトは同じにならない**。
    ///
    /// 「Latin は Hack だから見た目は変わらない」は**誤り**。 feature を
    /// 変えると桁は動く。 それを承知で切り替えるためにここに書いてある。
    #[test]
    fn switching_the_builtin_changes_latin_width() {
        const SAMPLE: &str = "Hello, sabitori 0123";
        let mut jp = wasm_like_shaper();
        let mut latin = TextShaper::with_fonts_only("ja", &[BUILTIN_FONT_LATIN.to_vec()]);
        let w_jp = measure(&mut jp, SAMPLE).size.width;
        let w_latin = measure(&mut latin, SAMPLE).size.width;

        assert!(w_jp < w_latin, "jp={w_jp} latin={w_latin}");
        let ratio = w_jp / w_latin;
        assert!(
            (0.85..0.90).contains(&ratio),
            "advance 比が想定 (0.527/0.602 ≒ 0.875) から外れた: {ratio:.3}"
        );
    }

    /// フォントが 0 個のときは、 cosmic-text の `no default font found` ではなく
    /// **直し方が書いてあるメッセージ**で落ちること。
    ///
    /// 落ちること自体は変わらない。 変えたのは、 依存クレート内部の 1 行から
    /// 「`fonts()` に何を書けばいいか」へ、 読む場所を寄せたところ。
    #[test]
    #[should_panic(expected = "fonts()")]
    fn an_empty_font_stack_says_what_to_write() {
        let mut s = empty_shaper();
        let _ = measure(&mut s, "これは落ちる");
    }

    /// 折り返し系のクエリ (`caret_pos` / `offset_at` / `range_rects`) も
    /// 同じ入口で止まること。 `measure_text` だけ直しても、 キャレットを
    /// 引いた瞬間に元の panic に戻るのでは意味がない。
    #[test]
    #[should_panic(expected = "fonts()")]
    fn the_caret_queries_stop_at_the_same_place() {
        let mut s = empty_shaper();
        let shape = TextShape {
            font_size: 16.0,
            bold: false,
            monospace: false,
            font_family: None,
            wrap_width: None,
            typo: Typography::default(),
        };
        let _ = s.caret_pos("abc", 1, shape);
    }

    /// アプリが渡したフォントが組み込みより先に来ること。
    ///
    /// 順序が逆だと、 CJK フォントをバンドルしても Hack が先に当たって
    /// **日本語が豆腐のまま直らない**。 バンドルの意味が消える。
    #[test]
    fn user_fonts_still_win_over_the_bundled_one() {
        let mut s = wasm_like_shaper();
        let jp = std::fs::read("../../assets/fonts/NotoSansJP-Regular.otf")
            .expect("リポジトリ同梱の JP フォント");
        s.prefer_user_fonts(&[jp]);
        let m = measure(&mut s, "日本語");
        assert!(m.size.width > 0.0, "{m:?}");
        assert!(
            s.font_system.db().len() >= 2,
            "user font と組み込みの両方が居ること"
        );
    }
}

#[cfg(test)]
mod caret_tests {
    use super::*;

    const EM: f32 = 16.0;
    /// 折り返しが確実に起きる幅。 実フォント依存なので、 行数そのものは
    /// assert せず「2 行以上ある」だけを前提にする。
    const NARROW: f32 = 120.0;
    const PARA: &str = "the quick brown fox jumps over the lazy dog";

    fn shaper() -> TextShaper {
        TextShaper::with_locale("ja")
    }

    fn wrapped() -> TextShape<'static> {
        TextShape::new(EM).wrap(NARROW)
    }

    /// **2 行目以降のキャレットが 1 行目に貼り付かないこと。**
    ///
    /// cosmic-text の `LayoutGlyph::start` は論理行の中での添字なので、 絶対値に
    /// 直し忘れると 2 行目の全オフセットが 1 行目の座標に潰れる。 1 行しか無い
    /// うちは同じ値なので、 この形でしか出ない。
    #[test]
    fn the_caret_moves_down_across_a_hard_line_break() {
        let mut s = shaper();
        let text = "first\nsecond";
        let shape = TextShape::new(EM);

        let a = s.caret_pos(text, 2, shape); // 1 行目の途中
        let b = s.caret_pos(text, 6 + 2, shape); // 2 行目の途中 ("second" の 's' + 2)

        assert_eq!(a.line, 0);
        assert_eq!(b.line, 1, "2 行目と認識されていない");
        assert!(b.y > a.y, "y が下がっていない: {} → {}", a.y, b.y);
        assert!(
            (a.x - b.x).abs() < a.x.max(b.x) * 0.9 + 1.0,
            "行内 x が同じ計算になっているか要確認 (a={}, b={})",
            a.x,
            b.x
        );
    }

    /// 改行の**直前**のキャレットは前の行の末尾に居ること。
    ///
    /// 次の行の先頭に飛ぶと、 行末で Backspace を押す位置が視覚的に合わない。
    #[test]
    fn the_caret_before_a_newline_stays_at_the_end_of_that_line() {
        let mut s = shaper();
        let text = "first\nsecond";
        let shape = TextShape::new(EM);

        let before_nl = s.caret_pos(text, 5, shape); // "first" の直後
        let after_nl = s.caret_pos(text, 6, shape); // "second" の先頭

        assert_eq!(before_nl.line, 0, "改行の手前は 1 行目");
        assert_eq!(after_nl.line, 1, "改行の直後は 2 行目");
        assert!(before_nl.x > 0.0, "1 行目の末尾なので x は 0 ではない");
        assert_eq!(after_nl.x, 0.0, "2 行目の先頭なので x は 0");
    }

    /// **空行にもキャレットが置けること。** グリフが無い視覚行を「先頭」に
    /// 潰すと、 改行を 2 回打った瞬間にキャレットが文頭へ飛ぶ。
    #[test]
    fn an_empty_line_still_has_a_caret_slot() {
        let mut s = shaper();
        let text = "a\n\nb";
        let shape = TextShape::new(EM);

        let empty = s.caret_pos(text, 2, shape); // 空行の位置
        let last = s.caret_pos(text, 3, shape); // "b" の手前

        assert_eq!(empty.line, 1, "空行が 2 行目として数えられていない");
        assert_eq!(empty.x, 0.0);
        assert_eq!(last.line, 2);
        assert!(last.y > empty.y);
    }

    /// 折り返し (ソフト改行) でも行が下がること。 `\n` が無くても複数行になる。
    #[test]
    fn wrapping_alone_produces_more_than_one_caret_line() {
        let mut s = shaper();
        let head = s.caret_pos(PARA, 0, wrapped());
        let tail = s.caret_pos(PARA, PARA.len(), wrapped());

        assert_eq!(head.line, 0);
        assert!(
            tail.line >= 1,
            "{NARROW}px 幅で折り返していない (行 {}) — 前提が崩れている",
            tail.line
        );
        assert!(tail.y > head.y);
    }

    /// **キャレットを置いた場所を読み返せること。** `caret_pos` と `offset_at`
    /// が食い違うと、 クリックした場所と違うところにカーソルが飛ぶ。
    #[test]
    fn offset_at_round_trips_with_caret_pos() {
        let mut s = shaper();
        let text = "first line\nsecond line";
        let shape = TextShape::new(EM);

        for &offset in &[0usize, 3, 10, 11, 14, text.len()] {
            let c = s.caret_pos(text, offset, shape);
            // 行の縦中央を突く (境界ちょうどだと隣の行に転びうる)。
            let back = s.offset_at(text, (c.x, c.y + c.line_height * 0.5), shape);
            assert_eq!(
                back, offset,
                "offset {offset} → ({}, {}) → {back} で戻ってこない",
                c.x, c.y
            );
        }
    }

    /// 範囲外の座標でも必ず答えること。 `Option` にすると欄の余白を
    /// クリックしたときに「何も起きない」になる。
    #[test]
    fn a_click_outside_the_text_still_lands_somewhere() {
        let mut s = shaper();
        let text = "abc\ndef";
        let shape = TextShape::new(EM);

        assert_eq!(s.offset_at(text, (-50.0, -50.0), shape), 0, "上に外れたら先頭");
        assert_eq!(
            s.offset_at(text, (9999.0, 9999.0), shape),
            text.len(),
            "下に外れたら末尾"
        );
        assert_eq!(
            s.offset_at(text, (9999.0, EM * 0.5), shape),
            3,
            "1 行目の右に外れたらその行の末尾"
        );
    }

    /// 選択範囲が**視覚行ごとに**割れること。 1 個の矩形で返ると行間まで
    /// 塗って隣の行に食い込む。
    #[test]
    fn a_selection_across_lines_yields_one_rect_per_line() {
        let mut s = shaper();
        let text = "first\nsecond\nthird";
        let shape = TextShape::new(EM);

        // "rst" + 改行 + "second" + 改行 + "th"
        let rects = s.range_rects(text, (2, 15), shape);
        assert_eq!(rects.len(), 3, "3 行にまたがるので 3 個: {rects:?}");
        assert!(rects[0].origin.y < rects[1].origin.y);
        assert!(rects[1].origin.y < rects[2].origin.y);
        assert!(rects.iter().all(|r| r.size.width > 0.0));
    }

    /// 空の選択は矩形を出さないこと。 幅 0 の矩形を出すと、 選択していない
    /// のに細い線が見える。
    #[test]
    fn an_empty_selection_draws_nothing() {
        let mut s = shaper();
        assert!(s.range_rects("abc", (1, 1), TextShape::new(EM)).is_empty());
    }

    /// 文字境界でないオフセットで panic しないこと。 日本語は 1 文字 3 バイトなので、
    /// バイト単位で動かす実装が途中で刻んでも落ちてはいけない。
    #[test]
    fn a_mid_character_offset_does_not_panic() {
        let mut s = shaper();
        let text = "あいう";
        let shape = TextShape::new(EM);
        for off in 0..=text.len() {
            let c = s.caret_pos(text, off, shape);
            assert!(c.x.is_finite() && c.y.is_finite());
        }
        assert!(!s.range_rects(text, (1, 5), shape).is_empty());
    }
}
