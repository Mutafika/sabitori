//! **画面外に描く** — 帳票・印刷・画像の書き出し
//! ([#75](https://github.com/Mutafika/sabitori/issues/75) の 12)。
//!
//! 窓を開かずに、ツリーをそのまま画像にする。業務アプリで「A4 の帳票を出す」に
//! なったとき、これが無いので**帳票だけ HTML を組んでブラウザの印刷に投げる**
//! ことになっていた。画面と帳票で書き方が 2 つに割れる。
//!
//! # 使い方
//!
//! ```ignore
//! // A4 縦、300dpi の PNG
//! let page = offscreen::render(&invoice_view(&order), Sheet::a4().dpi(300.0))?;
//! std::fs::write("請求書.png", page.to_png()?)?;
//! ```
//!
//! レイアウトは **CSS px と同じ論理 px** で回る。`dpi` は書き出す解像度だけを
//! 変えるので、**同じツリーが画面でも紙でも同じ形に組まれる** (72dpi でも
//! 300dpi でも行数は変わらない)。
//!
//! # 分かっていること
//!
//! - GPU が要る。取れなければ [`RenderError::NoGpu`]。
//! - 1 枚ぶん。複数ページに割るのはアプリ側 (どこで切るかは中身次第なので、
//!   フレームワークが決めると必ず外す)。
//! - PDF ではなく画像。印刷に投げるところは OS ごとの話なので、ここでは
//!   「正しい絵を作る」までを持つ。

use sabitori_core::element::Element;
use sabitori_core::Color;
use sabitori_gpu::wgpu;

use crate::bridge::{draw_ui_layer, MeasureCache, TextRendererMeasurer, UiDrawLists, UiRenderers};

/// 書き出す紙の指定。
#[derive(Clone, Copy, Debug)]
pub struct Sheet {
    /// レイアウトに使う幅 (論理 px、CSS px と同じ)。
    pub width: f32,
    /// レイアウトに使う高さ (論理 px)。
    pub height: f32,
    /// 書き出す解像度。96 が等倍 (CSS px = 画像 1px)。
    pub dpi: f32,
    /// 地の色。**既定は白** — 帳票を透明で書き出すと、印刷で黒い紙になる。
    pub background: Color,
}

/// 1 インチあたりの CSS px。紙の mm を論理 px に直す基準。
const CSS_DPI: f32 = 96.0;

impl Sheet {
    /// mm で紙を指定する (96dpi の論理 px に直す)。
    pub fn mm(width_mm: f32, height_mm: f32) -> Self {
        Self {
            width: width_mm / 25.4 * CSS_DPI,
            height: height_mm / 25.4 * CSS_DPI,
            dpi: CSS_DPI,
            background: Color::WHITE,
        }
    }

    /// 論理 px で指定する (画面の一部を切り出すとき)。
    pub fn px(width: f32, height: f32) -> Self {
        Self { width, height, dpi: CSS_DPI, background: Color::WHITE }
    }

    /// A4 縦 (210 × 297mm)。
    pub fn a4() -> Self {
        Self::mm(210.0, 297.0)
    }

    /// A4 横。
    pub fn a4_landscape() -> Self {
        Self::mm(297.0, 210.0)
    }

    /// B5 縦 (182 × 257mm)。納品書・帳票で使う。
    pub fn b5() -> Self {
        Self::mm(182.0, 257.0)
    }

    /// 書き出す解像度。印刷は 300 以上が目安。
    pub fn dpi(mut self, dpi: f32) -> Self {
        self.dpi = dpi.max(1.0);
        self
    }

    /// 地の色 (透過させたいなら `Color::TRANSPARENT`)。
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// 論理 px → 画像 px の倍率。
    pub fn scale(&self) -> f32 {
        self.dpi / CSS_DPI
    }

    /// 書き出される画像の大きさ (px)。
    pub fn pixel_size(&self) -> (u32, u32) {
        let s = self.scale();
        (
            ((self.width * s).round() as u32).max(1),
            ((self.height * s).round() as u32).max(1),
        )
    }
}

/// 書き出した画像 (RGBA8、上から下)。
#[derive(Clone, Debug)]
pub struct Rendered {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` バイト。
    pub rgba: Vec<u8>,
}

impl Rendered {
    /// PNG に符号化する。
    pub fn to_png(&self) -> Result<Vec<u8>, RenderError> {
        let buf = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .ok_or(RenderError::Encode)?;
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|_| RenderError::Encode)?;
        Ok(out.into_inner())
    }

    /// PNG として保存する。
    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> Result<(), RenderError> {
        std::fs::write(path, self.to_png()?).map_err(RenderError::Io)
    }

    /// 画素を読む (テスト用)。範囲外は `None`。
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        Some([self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]])
    }
}

/// 書き出しに失敗した理由。
#[derive(Debug)]
pub enum RenderError {
    /// GPU が取れなかった (CI のコンテナ、リモート端末など)。
    ///
    /// **握りつぶさずに返す** — 帳票が白紙で出てくるより、出せないと言うほうが
    /// まだ気づける。
    NoGpu,
    /// 読み戻しに失敗した。
    Readback,
    /// PNG への符号化に失敗した。
    Encode,
    Io(std::io::Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoGpu => write!(f, "GPU が取れないので画面外に描けない"),
            Self::Readback => write!(f, "描いた結果を読み戻せなかった"),
            Self::Encode => write!(f, "PNG に符号化できなかった"),
            Self::Io(e) => write!(f, "書き出しに失敗した: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// 画面外の device。窓 (surface) を持たない。
fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;
    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor { label: Some("sabitori_offscreen"), ..Default::default() },
        None,
    ))
    .ok()
}

/// ツリーを 1 枚の画像にする。
///
/// 文字は**実フォントで測ってから**組むので、折り返しも行数も画面と同じになる。
pub fn render(view: &Element, sheet: Sheet) -> Result<Rendered, RenderError> {
    let (device, queue) = headless_device().ok_or(RenderError::NoGpu)?;
    // 読み戻す前提なので、画面と同じ sRGB で描く。
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (px_w, px_h) = sheet.pixel_size();
    let scale = sheet.scale();

    let mut ui = sabitori_gpu::UiOverlayRenderer::new(&device, format);
    let mut text = sabitori_text::TextRenderer::new(&device, format, ui.globals_bind_group_layout());
    text.set_scale_factor(scale);
    // 実行時に積まれたフォント (#75 の 13) はここでも積む。入れないと、
    // **画面では出ている字が帳票だけ豆腐になる**。
    for font in crate::fonts::all() {
        text.load_font(font);
    }
    let mut images = sabitori_gpu::ImageRenderer::new(&device, format, ui.globals_bind_group_layout());
    let mut rings = sabitori_gpu::RingRenderer::new(&device, format, ui.globals_bind_group_layout());
    let mut lines = sabitori_gpu::LineRenderer::new(&device, format, ui.globals_bind_group_layout());

    // 組む。測り手を渡すので、画面と同じ折り返しになる。
    let build = {
        let cache = std::cell::RefCell::new(MeasureCache::new());
        let measurer = TextRendererMeasurer::new(&mut text, &cache);
        sabitori_core::build::build_tree_measured(view, sheet.width, sheet.height, &measurer)
    };

    let (base_rects, base_lists) = UiDrawLists::extract(&build.render_list, &mut text);
    let (overlay_rects, overlay_lists) = UiDrawLists::extract(&build.overlay_list, &mut text);

    ui.update_globals(&queue, sheet.width, sheet.height, scale);
    ui.upload_rects(&device, &queue, &base_rects, &overlay_rects);

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sabitori_offscreen_target"),
        size: wgpu::Extent3d { width: px_w, height: px_h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view_tex = texture.create_view(&Default::default());

    // base と overlay は**別の submit**にする。TextRenderer の glyph buffer は
    // submit ごとに 1 回しか書けないので、同じ submit に 2 層入れると
    // 片方の文字が消える (`UiOverlayRenderer` の doc)。
    let clear = wgpu::Color {
        r: sheet.background.r as f64,
        g: sheet.background.g as f64,
        b: sheet.background.b as f64,
        a: sheet.background.a as f64,
    };
    {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sabitori_offscreen_base"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            ui.draw_base(&mut pass);
            let mut r = UiRenderers {
                images: Some(&mut images),
                rings: Some(&mut rings),
                lines: Some(&mut lines),
                text: &mut text,
            };
            draw_ui_layer(&mut r, &base_lists, &device, &queue, &mut pass, ui.globals_bind_group());
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
    if !overlay_rects.is_empty() || !overlay_lists.is_empty() {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sabitori_offscreen_overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            ui.draw_overlay(&mut pass);
            let mut r = UiRenderers {
                images: Some(&mut images),
                rings: Some(&mut rings),
                lines: Some(&mut lines),
                text: &mut text,
            };
            draw_ui_layer(&mut r, &overlay_lists, &device, &queue, &mut pass, ui.globals_bind_group());
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    read_back(&device, &queue, &texture, px_w, px_h)
}

/// テクスチャを RGBA として読み戻す。
///
/// wgpu は行の先頭を 256 バイトに揃えることを要求するので、**幅によっては
/// 余白が挟まる**。揃えずに読むと、幅が 64px の倍数でない画像が斜めにずれる。
fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Rendered, RenderError> {
    const ALIGN: u32 = 256;
    let unpadded = width * 4;
    let padded = unpadded.div_ceil(ALIGN) * ALIGN;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sabitori_offscreen_readback"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    if device.poll(wgpu::Maintain::Wait).is_queue_empty() {
        // 走り切っている。
    }
    match rx.recv() {
        Ok(Ok(())) => {}
        _ => return Err(RenderError::Readback),
    }

    let mapped = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((unpadded * height) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        rgba.extend_from_slice(&mapped[start..start + unpadded as usize]);
    }
    drop(mapped);
    buffer.unmap();

    Ok(Rendered { width, height, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::element::{div, text, Px};

    /// GPU が無い環境 (CI のコンテナ) では飛ばす。
    macro_rules! gpu_or_skip {
        () => {
            if headless_device().is_none() {
                eprintln!("skip: GPU が無い");
                return;
            }
        };
    }

    #[test]
    fn a4_at_300dpi_is_the_right_number_of_pixels() {
        let sheet = Sheet::a4().dpi(300.0);
        let (w, h) = sheet.pixel_size();
        // 210mm / 25.4 * 300 = 2480、297mm / 25.4 * 300 = 3508 (JIS の A4 と同じ)。
        assert_eq!((w, h), (2480, 3508));

        // 論理 px はどの dpi でも同じ = 組み方が変わらない。
        assert_eq!(Sheet::a4().width, Sheet::a4().dpi(300.0).width);
    }

    /// **描いたものが実際に画素として出る。**
    #[test]
    fn a_filled_box_lands_in_the_image() {
        gpu_or_skip!();
        let view = div()
            .w(Px(200.0))
            .h(Px(100.0))
            .bg(Color::WHITE)
            .child(div().id("box").w(Px(50.0)).h(Px(50.0)).bg(Color::from_hex("#ff0000")));
        let out = render(&view, Sheet::px(200.0, 100.0)).expect("描けなかった");

        assert_eq!((out.width, out.height), (200, 100));
        let inside = out.pixel(25, 25).unwrap();
        assert!(inside[0] > 200 && inside[1] < 60, "赤い箱が出ていない: {inside:?}");
        let outside = out.pixel(150, 80).unwrap();
        assert!(outside[0] > 200 && outside[1] > 200, "地が白くない: {outside:?}");
    }

    /// **地の色は既定で白。** 透明のまま印刷に回すと紙が黒くなる。
    #[test]
    fn the_default_background_is_white() {
        gpu_or_skip!();
        let out = render(&div().w(Px(64.0)).h(Px(64.0)), Sheet::px(64.0, 64.0)).unwrap();
        assert_eq!(out.pixel(32, 32).unwrap(), [255, 255, 255, 255]);

        let out = render(
            &div().w(Px(64.0)).h(Px(64.0)),
            Sheet::px(64.0, 64.0).background(Color::TRANSPARENT),
        )
        .unwrap();
        assert_eq!(out.pixel(32, 32).unwrap()[3], 0, "透過を頼んだのに塗られている");
    }

    /// **幅が 64px の倍数でなくてもずれない** (読み戻しの行揃え)。
    #[test]
    fn an_odd_width_reads_back_without_skew() {
        gpu_or_skip!();
        // 左半分だけ黒。幅 101px は 256 バイト境界に乗らない。
        let view = div()
            .w(Px(101.0))
            .h(Px(20.0))
            .flex_row()
            .child(div().w(Px(50.0)).h(Px(20.0)).bg(Color::BLACK));
        let out = render(&view, Sheet::px(101.0, 20.0)).unwrap();

        for y in [3u32, 10, 16] {
            let left = out.pixel(10, y).unwrap();
            let right = out.pixel(90, y).unwrap();
            assert!(left[0] < 40, "{y} 行目の左が黒くない: {left:?}");
            assert!(right[0] > 200, "{y} 行目の右が白くない: {right:?}");
        }
    }

    /// 文字が出ること (実フォントで測って組んだ結果が画になる)。
    #[test]
    fn text_actually_paints() {
        gpu_or_skip!();
        let view = div()
            .w(Px(200.0))
            .h(Px(60.0))
            .p(Px(10.0))
            .child(text("請求書").font_size(24.0).color(Color::BLACK));
        let out = render(&view, Sheet::px(200.0, 60.0)).unwrap();

        let dark = (0..out.height)
            .flat_map(|y| (0..out.width).map(move |x| (x, y)))
            .filter(|&(x, y)| out.pixel(x, y).is_some_and(|p| p[0] < 128))
            .count();
        assert!(dark > 50, "文字が 1 画素も出ていない (暗い画素 {dark})");
    }

    /// **文字ごとに色が変わること** ([#78](https://github.com/Mutafika/sabitori/issues/78))。
    ///
    /// 1 要素のまま左半分が赤、右半分が青になる。ずれていたら「格子の色が
    /// 1 文字ずつずれる」形で出るので、実際の画素で見る。
    #[test]
    fn color_spans_paint_different_glyphs_differently() {
        gpu_or_skip!();
        // 等幅で 2 文字。前半 (1 バイト) を赤、後半を青に。
        let view = div().w(Px(120.0)).h(Px(40.0)).p(Px(4.0)).child(
            text("AB")
                .font_size(28.0)
                .mono()
                .color(Color::from_hex("#0000ff"))
                .color_spans([(0..1, Color::from_hex("#ff0000"))]),
        );
        let out = render(&view, Sheet::px(120.0, 40.0)).unwrap();

        // 画素を色ごとに数える (どこに出るかは書体次第なので、数で見る)。
        let mut red = 0;
        let mut blue = 0;
        for y in 0..out.height {
            for x in 0..out.width {
                let p = out.pixel(x, y).unwrap();
                if p[0] > 150 && p[2] < 100 {
                    red += 1;
                }
                if p[2] > 150 && p[0] < 100 {
                    blue += 1;
                }
            }
        }
        assert!(red > 10, "赤い文字が出ていない (赤 {red} 画素)");
        assert!(blue > 10, "青い文字が出ていない (青 {blue} 画素)");
    }

    /// PNG として保存できること。
    #[test]
    fn it_encodes_to_png() {
        gpu_or_skip!();
        let out = render(&div().w(Px(32.0)).h(Px(32.0)), Sheet::px(32.0, 32.0)).unwrap();
        let png = out.to_png().expect("符号化できない");
        assert_eq!(&png[1..4], b"PNG", "PNG の識別子が無い");
    }

    // -----------------------------------------------------------------
    // 半透明の合成。色は straight で渡し、rgb に alpha を掛けるのは
    // シェーダの 1 か所だけ。linear 0.5 は sRGB の 8bit で 188。
    // -----------------------------------------------------------------

    fn is_half(px: [u8; 4]) -> bool {
        px[..3].iter().all(|&c| (180..=196).contains(&c))
    }

    /// 回帰: 白 α0.5 を黒の上に置くと半分の明るさ。シェーダが自分の alpha を
    /// rgb に掛けていなかった頃は `1 + 0 × 0.5` で**真っ白** (255) になった。
    #[test]
    fn a_half_transparent_color_blends_to_half() {
        gpu_or_skip!();
        let view = div()
            .w(Px(64.0))
            .h(Px(64.0))
            .bg(Color::BLACK)
            .child(div().w(Px(64.0)).h(Px(64.0)).bg(Color::WHITE.with_alpha(0.5)));
        let out = render(&view, Sheet::px(64.0, 64.0)).unwrap();
        let px = out.pixel(32, 32).unwrap();
        assert!(is_half(px), "白 α0.5 on 黒が半分になっていない: {px:?}");
    }

    /// 回帰: `.opacity(0.5)` の黒い箱を白の上に置くと半分の明るさ。rgb と a の
    /// 両方に掛けたうえで a にもう一度掛けていた頃は a が 0.25 になり、白が
    /// 0.75 残って (225) 薄すぎた。
    #[test]
    fn opacity_fades_a_box_by_exactly_that_much() {
        gpu_or_skip!();
        let view = div()
            .w(Px(64.0))
            .h(Px(64.0))
            .bg(Color::WHITE)
            .child(div().w(Px(64.0)).h(Px(64.0)).bg(Color::BLACK).opacity(0.5));
        let out = render(&view, Sheet::px(64.0, 64.0)).unwrap();
        let px = out.pixel(32, 32).unwrap();
        assert!(is_half(px), "opacity 0.5 の黒 on 白が半分になっていない: {px:?}");
    }

    /// 半透明の色に opacity を重ねると、alpha が掛け算で効く (0.5 × 0.5)。
    #[test]
    fn opacity_multiplies_into_a_translucent_color() {
        gpu_or_skip!();
        let view = div()
            .w(Px(64.0))
            .h(Px(64.0))
            .bg(Color::BLACK)
            .child(
                div()
                    .w(Px(64.0))
                    .h(Px(64.0))
                    .bg(Color::WHITE.with_alpha(0.5))
                    .opacity(0.5),
            );
        let out = render(&view, Sheet::px(64.0, 64.0)).unwrap();
        let px = out.pixel(32, 32).unwrap();
        // linear 0.25 → sRGB 137
        assert!(
            px[..3].iter().all(|&c| (129..=145).contains(&c)),
            "0.5 × 0.5 になっていない: {px:?}"
        );
    }

    // -----------------------------------------------------------------
    // 等幅の文字の格子 (#102)
    // -----------------------------------------------------------------

    fn grid_of(cols: usize, marks: &[usize]) -> sabitori_core::CellGrid {
        use sabitori_core::{CellFlags, CellGrid, GridCell};
        let red = Color::from_hex("#ff0000");
        let mut g = CellGrid::new(cols, 1, Color::BLACK);
        for &c in marks {
            g.set(c, 0, GridCell { ch: 'X', fg: Color::BLACK, bg: Some(red), flags: CellFlags::NONE });
        }
        g
    }

    /// **字は行末まで格子に乗る。** `text()` の run はシェープした字送りで並ぶので
    /// セル幅と少しずつずれ、行末ほど背景から離れていた (mearie は 8 セルごとに
    /// 切って置き直していた)。格子は `col * cell_w` に置く。
    #[test]
    fn cell_grid_glyphs_stay_inside_their_cells_to_the_end_of_the_line() {
        gpu_or_skip!();
        // 字の既定の字送りと合わない幅 (8.4) にして、ずれが出る条件にする。
        let (cw, ch) = (8.4_f32, 18.0_f32);
        let marks = [0, 40, 79];
        let view = div().w(Px(700.0)).h(Px(40.0)).child(
            sabitori_core::cell_grid(std::sync::Arc::new(grid_of(80, &marks)), cw, ch).font_size(14.0),
        );
        let out = render(&view, Sheet::px(700.0, 40.0)).expect("描けなかった");
        let mut inked = std::collections::BTreeSet::new();
        for y in 0..18u32 {
            for x in 0..700u32 {
                let p = out.pixel(x, y).unwrap();
                if p[0] < 120 && p[1] < 120 {
                    // 字の墨。どのセルに落ちたか。
                    inked.insert((x as f32 / cw).floor() as usize);
                }
            }
        }
        assert_eq!(inked.into_iter().collect::<Vec<_>>(), marks.to_vec(), "字がセルの外に出ている");
        // 背景もその場所に。
        let red = out.pixel((79.5 * cw) as u32, 2).unwrap();
        assert!(red[0] > 200 && red[1] < 60, "行末の背景が無い: {red:?}");
    }

    fn text_renderer() -> Option<sabitori_text::TextRenderer> {
        let (device, _queue) = headless_device()?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let ui = sabitori_gpu::UiOverlayRenderer::new(&device, format);
        Some(sabitori_text::TextRenderer::new(&device, format, ui.globals_bind_group_layout()))
    }

    /// **版番号が同じ行は組み直さない** — 前の字形をそのまま使う。
    /// 変えたら版を上げる、が約束 (`CellGrid::set` は自動で上げる)。
    #[test]
    fn rows_with_the_same_version_reuse_last_frames_glyphs() {
        gpu_or_skip!();
        let mut tr = text_renderer().unwrap();
        let mut g = grid_of(10, &[1, 2]);
        let draw = |tr: &mut sabitori_text::TextRenderer, g: &sabitori_core::CellGrid, x: f32| {
            tr.prepare_cell_grid(g, x, 0.0, 8.0, 18.0, 14.0, 1.0, None, Some(7))
        };
        let first = draw(&mut tr, &g, 0.0);
        assert_eq!(first.len(), 2);

        // 版を上げずに消す → 前の字形のまま (使い回している証拠)。
        g.cells[2].ch = ' ';
        assert_eq!(draw(&mut tr, &g, 0.0).len(), 2);
        // 位置だけ変わっても使い回す (格子ごと動かす)。
        let moved = draw(&mut tr, &g, 100.0);
        assert_eq!(moved[0].position[0], first[0].position[0] + 100.0);

        // 版を上げれば組み直す。
        g.row_versions[0] += 1;
        assert_eq!(draw(&mut tr, &g, 0.0).len(), 1);

        // 鍵が無ければ毎回組む。
        g.cells[1].ch = ' ';
        assert_eq!(tr.prepare_cell_grid(&g, 0.0, 0.0, 8.0, 18.0, 14.0, 1.0, None, None).len(), 0);
    }

    /// 字の大きさ・セルの寸法・不透明度が変われば、版が同じでも組み直す。
    #[test]
    fn changing_the_metrics_rebuilds_the_row() {
        gpu_or_skip!();
        let mut tr = text_renderer().unwrap();
        let g = grid_of(10, &[3]);
        let a = tr.prepare_cell_grid(&g, 0.0, 0.0, 8.0, 18.0, 14.0, 1.0, None, Some(1));
        let b = tr.prepare_cell_grid(&g, 0.0, 0.0, 10.0, 18.0, 14.0, 1.0, None, Some(1));
        assert!((b[0].position[0] - a[0].position[0] - 6.0).abs() < 0.01, "3 列目 × 2px");
        let c = tr.prepare_cell_grid(&g, 0.0, 0.0, 10.0, 18.0, 14.0, 0.5, None, Some(1));
        assert!((c[0].color[3] - 0.5).abs() < 0.01);
    }
}
