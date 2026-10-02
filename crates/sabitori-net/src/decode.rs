//! Decode image bytes into a Sabitori `ImageData` (RGBA8).

use std::io::Cursor;

use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use sabitori_core::element::ImageData;
use sabitori_core::image_cache::fit_within;

/// Decode PNG / JPEG / GIF / WebP bytes into an RGBA8 `ImageData`.
///
/// Returns an error string suitable for putting straight into
/// [`crate::CacheState::Failed`].
pub fn decode_image(bytes: &[u8]) -> Result<ImageData, String> {
    decode_image_max(bytes, None)
}

/// [`decode_image`] して、長い辺が `max_px` を超えていれば縮める
/// ([#114](https://github.com/Mutafika/sabitori/issues/114))。拡大はしない。
///
/// 縮めるのは RGBA へ広げる**前**。4032×3024 の写真を 400px にするとき、
/// 48MB の RGBA を一度作ってから縮める、ということをしない。
pub fn decode_image_max(bytes: &[u8], max_px: Option<u32>) -> Result<ImageData, String> {
    let mut decoder = reader(bytes)?.into_decoder().map_err(|e| format!("decode: {e}"))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| format!("decode: {e}"))?;
    // スマホの写真は横長で撮って「縦に回して見せる」印 (EXIF) を付けていることが
    // 多い。ブラウザは `<img>` でも `createImageBitmap` でもこれに従うので、
    // native も従わないと web と native で写真の向きが変わる。
    img.apply_orientation(orientation);
    if let Some(max_px) = max_px {
        let (w, h) = img.dimensions();
        let (tw, th) = fit_within(w, h, max_px);
        if (tw, th) != (w, h) {
            // `thumbnail` は面積平均で縮める。写真の縮小なら Lanczos と見分けが
            // つかず、ずっと速い。
            img = img.thumbnail_exact(tw, th);
        }
    }
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();
    Ok(ImageData::new(rgba.into_raw(), w, h))
}

fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, String> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("decode: {e}"))
}

/// 見えるときの大きさ (向きの印を当てた後)。ヘッダだけ読むので軽い。
///
/// web で `createImageBitmap` に縮めた大きさを渡すのに、先に元の大きさが要る。
/// ブラウザは向きを当ててから縮めるので、90° 回す印なら幅と高さを入れ替える。
pub fn displayed_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    let mut decoder = reader(bytes)?.into_decoder().map_err(|e| format!("decode: {e}"))?;
    let (w, h) = decoder.dimensions();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    Ok(if swaps_axes(orientation) { (h, w) } else { (w, h) })
}

fn swaps_axes(o: Orientation) -> bool {
    matches!(
        o,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// ブラウザに読ませる (web 用、[#114](https://github.com/Mutafika/sabitori/issues/114))。
///
/// `decode_image_max` を wasm で呼ぶと画面と同じスレッドで走り、4032×3024 の
/// JPEG 1 枚で 150ms ほど画面が止まる。`createImageBitmap` は読み込みと縮小を
/// 画面のスレッドの外でやるので、こちらに残るのは縮めた後の画素を取り出す分だけ。
///
/// ブラウザで読めなかったとき (古いブラウザで `OffscreenCanvas` が無い等) は
/// `decode_image_max` に落ちる。止まりはするが、絵は出る。
#[cfg(target_arch = "wasm32")]
pub async fn decode_image_in_browser(bytes: &[u8], max_px: Option<u32>) -> Result<ImageData, String> {
    match browser::decode(bytes, max_px).await {
        Ok(data) => Ok(data),
        Err(_) => decode_image_max(bytes, max_px),
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        Blob, ColorSpaceConversion, ImageBitmap, ImageBitmapOptions, ImageOrientation,
        OffscreenCanvas, OffscreenCanvasRenderingContext2d, PremultiplyAlpha, ResizeQuality,
    };

    pub(super) async fn decode(bytes: &[u8], max_px: Option<u32>) -> Result<ImageData, JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let blob = Blob::new_with_u8_array_sequence(&parts)?;

        let opts = ImageBitmapOptions::new();
        opts.set_image_orientation(ImageOrientation::FromImage);
        // 画素を素の RGBA で取り出すので、ブラウザ側でも premultiply しない。
        opts.set_premultiply_alpha(PremultiplyAlpha::None);
        opts.set_color_space_conversion(ColorSpaceConversion::Default);
        // ヘッダを読めない形式 (ブラウザだけが読める AVIF 等) は縮めずに読ませる。
        if let (Some(max_px), Ok((w, h))) = (max_px, displayed_dimensions(bytes)) {
            let (tw, th) = fit_within(w, h, max_px);
            if (tw, th) != (w, h) {
                opts.set_resize_width(tw);
                opts.set_resize_height(th);
                opts.set_resize_quality(ResizeQuality::High);
            }
        }
        let promise = window.create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &opts)?;
        let bitmap: ImageBitmap = JsFuture::from(promise).await?.dyn_into()?;
        let result = read_pixels(&bitmap);
        bitmap.close();
        result
    }

    /// ImageBitmap の画素を取り出す。ここだけは画面のスレッドで走るが、
    /// 縮めた後の大きさぶんしか触らない。
    fn read_pixels(bitmap: &ImageBitmap) -> Result<ImageData, JsValue> {
        // 大きさは実物から取る (向きの扱いがブラウザとずれても縦横比は崩れない)。
        let (w, h) = (bitmap.width(), bitmap.height());
        let canvas = OffscreenCanvas::new(w, h)?;
        // 一度読むだけなので GPU に置かせない (置くと読み戻しが要る)。
        let ctx_opts = js_sys::Object::new();
        js_sys::Reflect::set(&ctx_opts, &"willReadFrequently".into(), &JsValue::TRUE)?;
        let ctx: OffscreenCanvasRenderingContext2d = canvas
            .get_context_with_context_options("2d", &ctx_opts)?
            .ok_or("no 2d context")?
            .dyn_into()?;
        ctx.draw_image_with_image_bitmap(bitmap, 0.0, 0.0)?;
        let data = ctx.get_image_data(0.0, 0.0, w as f64, h as f64)?;
        Ok(ImageData::new(data.data().0, w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 40, 10, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn shrinks_the_long_side_to_the_limit() {
        let d = decode_image_max(&png(800, 600), Some(200)).unwrap();
        assert_eq!((d.width, d.height), (200, 150));
        assert_eq!(d.rgba.len(), 200 * 150 * 4);
        // 色は縮めても保たれる (単色なので平均しても同じ)。
        assert_eq!(&d.rgba[..4], &[200, 40, 10, 255]);
    }

    /// 「90° 右に回して見せる」印 (EXIF Orientation = 6) 付きの PNG。
    /// 左半分が赤、右半分が青。
    fn png_rotated_90(w: u32, h: u32) -> Vec<u8> {
        use image::ImageEncoder;
        let img = image::RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) }
        });
        let exif = vec![
            b'M', b'M', 0, 42, 0, 0, 0, 8, // TIFF ヘッダ (big endian)、IFD は 8 バイト目
            0, 1, // 項目 1 つ
            0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, // Orientation (SHORT) = 6
            0, 0, 0, 0, // 次の IFD は無い
        ];
        let mut out = Vec::new();
        let mut enc = image::codecs::png::PngEncoder::new(&mut out);
        enc.set_exif_metadata(exif).unwrap();
        enc.write_image(&img, w, h, image::ExtendedColorType::Rgba8).unwrap();
        out
    }

    /// 横で撮って縦に見せる写真は、web (ブラウザが向きを当てる) と同じく縦で出る。
    #[test]
    fn follows_the_exif_orientation_like_browsers_do() {
        let bytes = png_rotated_90(80, 40);
        assert_eq!(displayed_dimensions(&bytes).unwrap(), (40, 80));
        let d = decode_image(&bytes).unwrap();
        assert_eq!((d.width, d.height), (40, 80));
        // 右に 90° 回すと、左半分 (赤) が上に来る。
        assert_eq!(&d.rgba[..4], &[255, 0, 0, 255]);
        let last = d.rgba.len() - 4;
        assert_eq!(&d.rgba[last..], &[0, 0, 255, 255]);
        // 縮めるのは向きを当てた後の長い辺。
        let d = decode_image_max(&bytes, Some(20)).unwrap();
        assert_eq!((d.width, d.height), (10, 20));
    }

    #[test]
    fn small_images_are_left_alone() {
        let d = decode_image_max(&png(64, 48), Some(200)).unwrap();
        assert_eq!((d.width, d.height), (64, 48));
    }

    #[test]
    fn no_limit_is_full_size() {
        let d = decode_image(&png(300, 100)).unwrap();
        assert_eq!((d.width, d.height), (300, 100));
    }
}
