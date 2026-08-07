//! Decode image bytes into a Sabitori `ImageData` (RGBA8).

use image::GenericImageView;
use sabitori_core::element::ImageData;

/// Decode PNG / JPEG / GIF / WebP bytes into an RGBA8 `ImageData`.
///
/// Returns an error string suitable for putting straight into
/// [`crate::CacheState::Failed`].
pub fn decode_image(bytes: &[u8]) -> Result<ImageData, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("decode: {e}"))?;
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();
    Ok(ImageData::new(rgba.into_raw(), w, h))
}
