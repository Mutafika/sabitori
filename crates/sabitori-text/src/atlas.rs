use cosmic_text::{CacheKey, SwashCache, SwashContent};
use etagere::{size2, AllocId, BucketedAtlasAllocator};
use std::collections::HashMap;

const ATLAS_SIZE: i32 = 2048;

/// Cached info about a glyph in the atlas.
#[derive(Clone, Copy, Debug)]
pub struct GlyphEntry {
    pub alloc_id: AllocId,
    /// UV coordinates in the atlas (0..1 range).
    pub uv_rect: [f32; 4], // x, y, w, h in UV space
    /// Size of the glyph in pixels.
    pub width: f32,
    pub height: f32,
    /// Offset from the glyph origin to the top-left of the bitmap.
    pub offset_x: f32,
    pub offset_y: f32,
    /// `true` when this glyph is a color bitmap (emoji: `SwashContent::Color`)
    /// rather than an alpha mask. The shader must output the atlas RGBA
    /// directly for these instead of tinting by the text color.
    pub color: bool,
}

/// GPU glyph atlas using shelf-packing.
pub struct GlyphAtlas {
    allocator: BucketedAtlasAllocator,
    cache: HashMap<CacheKey, GlyphEntry>,
    pub pixels: Vec<u8>, // RGBA
    pub size: u32,
    /// Rows of `pixels` written since the last upload, as an **inclusive**
    /// `min_y..=max_y` band. `None` = clean, nothing to send.
    ///
    /// A row band rather than a bool: uploading the whole 2048² texture costs
    /// 16 MiB every time a single glyph is added, which for a typical frame is
    /// ~320× the glyph data itself. Japanese text keeps producing unseen
    /// characters, so this fired continuously rather than settling down.
    ///
    /// A band rather than a rect, because `pixels` is row-major — a row range
    /// is one contiguous slice and therefore one `write_texture`. Tracking an x
    /// range too would need a per-row copy or a strided upload, and would buy
    /// little: the shelf allocator scatters a frame's new glyphs across the
    /// width, so the x span is usually near-full anyway.
    dirty_rows: Option<(u32, u32)>,
    /// Set when an `allocate` fails because the atlas is full (a glyph was
    /// dropped). The atlas has no eviction, so a long session accumulates
    /// stale glyphs (old numbers, closed panels, sub-pixel bin variants) until
    /// it overflows and new glyphs silently render as blanks. The renderer
    /// watches this flag and flushes (`clear`) + re-shapes on the next frame so
    /// only the currently-visible glyph set re-fills — self-healing instead of
    /// staying broken. Reset by `clear`.
    pub exhausted: bool,
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self::with_size(ATLAS_SIZE as u32)
    }

    /// Build an atlas of the given square size. Production uses [`ATLAS_SIZE`]
    /// via [`GlyphAtlas::new`]; tests use a tiny atlas to exercise the
    /// exhaustion / self-heal path without rasterizing thousands of glyphs.
    pub fn with_size(size: u32) -> Self {
        let s = size as i32;
        Self {
            allocator: BucketedAtlasAllocator::new(size2(s, s)),
            cache: HashMap::new(),
            pixels: vec![0; size as usize * size as usize * 4],
            size,
            // Fully dirty to start: the texture has never been written, so the
            // first upload establishes it. One 16 MiB transfer per atlas, not
            // per frame.
            dirty_rows: Some((0, size - 1)),
            exhausted: false,
        }
    }

    /// Rows pending upload as an inclusive `(min_y, max_y)`, or `None` when
    /// there is nothing to send. The renderer copies exactly this band and then
    /// calls [`GlyphAtlas::mark_uploaded`].
    pub fn dirty_rows(&self) -> Option<(u32, u32)> {
        self.dirty_rows
    }

    /// Clear the pending band. Call **after** the rows have actually been
    /// written to the texture — clearing early drops glyphs on the floor and
    /// they render as blanks until something else happens to re-dirty them.
    pub fn mark_uploaded(&mut self) {
        self.dirty_rows = None;
    }

    /// Widen the pending band to include `y .. y + height`.
    ///
    /// Union, never replace: several glyphs can land between two uploads, and
    /// overwriting the band would strand the earlier ones on the CPU side.
    fn mark_rows_dirty(&mut self, y: u32, height: u32) {
        if height == 0 {
            return;
        }
        let last = self.size.saturating_sub(1);
        let (lo, hi) = (y.min(last), (y + height - 1).min(last));
        self.dirty_rows = Some(match self.dirty_rows {
            Some((min_y, max_y)) => (min_y.min(lo), max_y.max(hi)),
            None => (lo, hi),
        });
    }

    /// Drop every cached glyph and reset the allocator (the pixel buffer is
    /// zeroed; `dirty` forces the next prepare to re-upload it). Called when
    /// the rasterization scale factor changes: the scale is baked into each
    /// `CacheKey`, so old-scale bitmaps become unreachable — without a flush
    /// they squat in the fixed-size atlas until allocation fails and new
    /// glyphs silently render as blanks.
    pub fn clear(&mut self) {
        let s = self.size as i32;
        self.allocator = BucketedAtlasAllocator::new(size2(s, s));
        self.cache.clear();
        self.pixels.fill(0);
        // Every row, not just the band that was pending: `fill(0)` wiped rows
        // the GPU still holds glyphs for. Marking only the pending band would
        // leave those stale bitmaps on the texture while the CPU thinks they
        // are gone — the freed slots get reallocated and two glyphs blend.
        self.dirty_rows = Some((0, self.size - 1));
        self.exhausted = false;
    }

    /// Get or rasterize a glyph into the atlas.
    pub fn get_or_insert(
        &mut self,
        cache_key: CacheKey,
        swash_cache: &mut SwashCache,
        font_system: &mut cosmic_text::FontSystem,
    ) -> Option<GlyphEntry> {
        if let Some(entry) = self.cache.get(&cache_key) {
            return Some(*entry);
        }

        let image = swash_cache.get_image(font_system, cache_key).as_ref()?;
        let width = image.placement.width as i32;
        let height = image.placement.height as i32;

        if width == 0 || height == 0 {
            return None;
        }

        // Allocate space in the atlas (+2 for 1px padding). A `None` means the
        // atlas is full: flag it so the renderer flushes + re-shapes next frame
        // (self-heal), and drop this glyph for the current frame.
        let alloc = match self.allocator.allocate(size2(width + 2, height + 2)) {
            Some(a) => a,
            None => {
                self.exhausted = true;
                return None;
            }
        };

        let atlas_x = alloc.rectangle.min.x + 1;
        let atlas_y = alloc.rectangle.min.y + 1;

        // Copy glyph bitmap into atlas
        let atlas_size = self.size as usize;
        match image.content {
            SwashContent::Mask => {
                // Single-channel alpha mask → RGBA
                for py in 0..height as usize {
                    for px in 0..width as usize {
                        let src_idx = py * width as usize + px;
                        let dst_x = atlas_x as usize + px;
                        let dst_y = atlas_y as usize + py;
                        let dst_idx = (dst_y * atlas_size + dst_x) * 4;
                        let alpha = image.data[src_idx];
                        self.pixels[dst_idx] = 255;
                        self.pixels[dst_idx + 1] = 255;
                        self.pixels[dst_idx + 2] = 255;
                        self.pixels[dst_idx + 3] = alpha;
                    }
                }
            }
            SwashContent::Color => {
                // RGBA color glyphs (emoji)
                for py in 0..height as usize {
                    for px in 0..width as usize {
                        let src_idx = (py * width as usize + px) * 4;
                        let dst_x = atlas_x as usize + px;
                        let dst_y = atlas_y as usize + py;
                        let dst_idx = (dst_y * atlas_size + dst_x) * 4;
                        self.pixels[dst_idx] = image.data[src_idx];
                        self.pixels[dst_idx + 1] = image.data[src_idx + 1];
                        self.pixels[dst_idx + 2] = image.data[src_idx + 2];
                        self.pixels[dst_idx + 3] = image.data[src_idx + 3];
                    }
                }
            }
            SwashContent::SubpixelMask => {
                // RGB subpixel → treat as grayscale for now
                for py in 0..height as usize {
                    for px in 0..width as usize {
                        let src_idx = (py * width as usize + px) * 3;
                        let dst_x = atlas_x as usize + px;
                        let dst_y = atlas_y as usize + py;
                        let dst_idx = (dst_y * atlas_size + dst_x) * 4;
                        let r = image.data[src_idx];
                        let g = image.data[src_idx + 1];
                        let b = image.data[src_idx + 2];
                        let avg = ((r as u16 + g as u16 + b as u16) / 3) as u8;
                        self.pixels[dst_idx] = 255;
                        self.pixels[dst_idx + 1] = 255;
                        self.pixels[dst_idx + 2] = 255;
                        self.pixels[dst_idx + 3] = avg;
                    }
                }
            }
        }

        // Only the glyph's own rows were touched. The 1px padding ring around
        // the allocation is never written — it stays zero from `new` / `clear`
        // — so it does not belong in the band.
        self.mark_rows_dirty(atlas_y as u32, height as u32);

        let inv = 1.0 / self.size as f32;
        let entry = GlyphEntry {
            alloc_id: alloc.id,
            uv_rect: [
                atlas_x as f32 * inv,
                atlas_y as f32 * inv,
                width as f32 * inv,
                height as f32 * inv,
            ],
            width: width as f32,
            height: height as f32,
            offset_x: image.placement.left as f32,
            offset_y: image.placement.top as f32,
            color: matches!(image.content, SwashContent::Color),
        };

        self.cache.insert(cache_key, entry);
        Some(entry)
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};

    /// Rasterize every glyph of `text` at `size` into `atlas`, returning how
    /// many were actually inserted (a full atlas drops them → returns `None`).
    fn insert(
        atlas: &mut GlyphAtlas,
        fs: &mut FontSystem,
        sc: &mut SwashCache,
        text: &str,
        size: f32,
    ) -> usize {
        let metrics = Metrics::new(size, size * 1.4);
        let mut buffer = Buffer::new(fs, metrics);
        buffer.set_size(fs, Some(f32::MAX), None);
        buffer.set_text(fs, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(fs, false);
        let mut n = 0;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                if atlas.get_or_insert(physical.cache_key, sc, fs).is_some() {
                    n += 1;
                }
            }
        }
        n
    }

    /// The whole point of the band: adding one small glyph must not mark the
    /// whole texture. This is the regression that cost 16 MiB per dirty frame
    /// ([#48](https://github.com/Mutafika/sabitori/issues/48)) — ~320× the
    /// frame's actual glyph data.
    #[test]
    fn inserting_a_glyph_marks_only_its_own_rows() {
        let mut fs = FontSystem::new();
        let mut sc = SwashCache::new();
        let mut atlas = GlyphAtlas::new();

        // A fresh atlas is fully dirty — the texture has never been written.
        assert_eq!(atlas.dirty_rows(), Some((0, atlas.size - 1)));
        atlas.mark_uploaded();
        assert_eq!(atlas.dirty_rows(), None, "mark_uploaded must clear the band");

        assert!(insert(&mut atlas, &mut fs, &mut sc, "A", 12.0) >= 1);
        let (min_y, max_y) = atlas
            .dirty_rows()
            .expect("inserting a glyph must mark rows dirty");
        let rows = max_y - min_y + 1;
        assert!(
            rows < atlas.size / 8,
            "a 12px glyph marked {rows} rows of {} — band is not tracking",
            atlas.size
        );
    }

    /// The band is a **union**. Two glyphs landing on different shelves between
    /// uploads must both be covered; replacing the band instead of widening it
    /// would strand the earlier glyph on the CPU and render it blank.
    #[test]
    fn a_second_glyph_widens_the_band_instead_of_replacing_it() {
        let mut fs = FontSystem::new();
        let mut sc = SwashCache::new();
        let mut atlas = GlyphAtlas::new();
        atlas.mark_uploaded();

        assert!(insert(&mut atlas, &mut fs, &mut sc, "A", 12.0) >= 1);
        let first = atlas.dirty_rows().expect("first insert must dirty rows");
        // Much taller → a different shelf, i.e. different rows.
        assert!(insert(&mut atlas, &mut fs, &mut sc, "B", 96.0) >= 1);
        let both = atlas.dirty_rows().expect("second insert must dirty rows");

        assert!(
            both.0 <= first.0 && both.1 >= first.1,
            "band {both:?} dropped the first glyph's rows {first:?}"
        );
        assert!(
            both.1 - both.0 > first.1 - first.0,
            "the taller glyph should have widened the band: {first:?} -> {both:?}"
        );
    }

    /// The band must cover **every** row that actually changed. Scanning the
    /// pixel buffer is the direct check, and the one that catches an off-by-one
    /// at either edge: a written row outside the band is a glyph the GPU never
    /// receives, which renders as a blank box.
    ///
    /// Works because the `Mask` branch writes `255,255,255,alpha` across the
    /// glyph's full box — even fully transparent pixels leave RGB non-zero — so
    /// "row contains a non-zero byte" is exactly "row was written".
    #[test]
    fn the_band_covers_every_written_row() {
        let mut fs = FontSystem::new();
        let mut sc = SwashCache::new();
        let mut atlas = GlyphAtlas::new();
        atlas.mark_uploaded();

        // Two very different heights → two shelves, far apart in y.
        assert!(insert(&mut atlas, &mut fs, &mut sc, "日本語テキスト", 14.0) >= 1);
        assert!(insert(&mut atlas, &mut fs, &mut sc, "Wg", 64.0) >= 1);
        let (min_y, max_y) = atlas.dirty_rows().expect("inserts must dirty rows");

        let stride = atlas.size as usize * 4;
        let mut written_rows = 0;
        for y in 0..atlas.size as usize {
            if atlas.pixels[y * stride..(y + 1) * stride]
                .iter()
                .any(|&b| b != 0)
            {
                written_rows += 1;
                assert!(
                    y as u32 >= min_y && y as u32 <= max_y,
                    "row {y} was written but sits outside the band ({min_y}..={max_y})"
                );
            }
        }
        assert!(written_rows > 0, "the test inserted nothing");
        // And the band should be tight, not a stealth full-texture upload.
        assert!(
            max_y - min_y + 1 < atlas.size / 8,
            "band {}..={max_y} spans {} of {} rows",
            min_y,
            max_y - min_y + 1,
            atlas.size
        );
    }

    /// `clear` zeroes every pixel, so every row must go back to the GPU — not
    /// just whatever band happened to be pending. Marking less leaves stale
    /// bitmaps on the texture after a self-heal flush, and the reallocated
    /// slots then blend two glyphs together.
    #[test]
    fn clear_marks_every_row_dirty() {
        let mut atlas = GlyphAtlas::with_size(128);
        atlas.mark_uploaded();
        assert_eq!(atlas.dirty_rows(), None);

        atlas.clear();
        assert_eq!(atlas.dirty_rows(), Some((0, 127)));
    }

    /// A full atlas must flag `exhausted` (so the renderer knows to flush)
    /// rather than silently dropping glyphs forever, and `clear` must reset it
    /// so the visible glyph set re-fills — the self-heal contract that fixes
    /// "the UI text sometimes goes blank after a long session".
    #[test]
    fn exhausts_then_self_heals() {
        let mut fs = FontSystem::new();
        let mut sc = SwashCache::new();
        // Tiny atlas: a few dozen glyphs overflow it (no need to shape
        // thousands to fill the real 2048²).
        let mut atlas = GlyphAtlas::with_size(128);
        assert!(!atlas.exhausted);

        // Same letters at many sizes → many distinct cache keys → overflow.
        let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        for size in 8..48 {
            insert(&mut atlas, &mut fs, &mut sc, text, size as f32);
            if atlas.exhausted {
                break;
            }
        }
        assert!(
            atlas.exhausted,
            "a filled fixed-size atlas must flag exhaustion"
        );

        // Self-heal: clear frees the space and resets the flag; fresh inserts
        // succeed again (the currently-visible glyphs re-rasterize).
        atlas.clear();
        assert!(!atlas.exhausted);
        assert_eq!(atlas.cache.len(), 0);
        let n = insert(&mut atlas, &mut fs, &mut sc, "AB", 12.0);
        assert!(n >= 1, "a cleared atlas must accept glyphs again");
    }
}
