use image::RgbaImage;
use crate::sprite::sheet::Frame;

/// Copy a source frame region into `dst`, applying an integer upscale and
/// optional horizontal flip, while premultiplying alpha for Win32
/// `UpdateLayeredWindow` (which requires premultiplied BGRA).
///
/// `dst` must be exactly `w * scale` × `h * scale` pixels.
#[allow(clippy::too_many_arguments)]
pub fn blit_frame(
    src: &RgbaImage,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
    dst: &mut Vec<u8>, // BGRA output for Win32 DIBSection
    scale: f32,
    flip_h: bool,
) {
    let dw = (src_w as f32 * scale).round() as u32;
    let dh = (src_h as f32 * scale).round() as u32;
    dst.resize((dw * dh * 4) as usize, 0);

    for dy in 0..dh {
        let sy = ((dy as f32 / scale) as u32).min(src_h - 1);
        for dx in 0..dw {
            let raw_sx = ((dx as f32 / scale) as u32).min(src_w - 1);
            let sx = if flip_h { src_w - 1 - raw_sx } else { raw_sx };

            let px = src.get_pixel(src_x + sx, src_y + sy);
            let [r, g, b, a] = px.0;

            // Premultiply.
            let pm = |c: u8| -> u8 { (c as u32 * a as u32 / 255) as u8 };
            let pr = pm(r);
            let pg = pm(g);
            let pb = pm(b);

            let dst_idx = ((dy * dw + dx) * 4) as usize;
            dst[dst_idx] = pb;     // B
            dst[dst_idx + 1] = pg; // G
            dst[dst_idx + 2] = pr; // R
            dst[dst_idx + 3] = a;  // A (premultiplied BGRA)
        }
    }
}

/// Pre-computed scaled + premultiplied BGRA buffers for every frame in a sprite sheet.
///
/// Both normal and horizontally-flipped variants are stored so that render_cached_frame
/// can do a plain memcpy instead of per-pixel work each tick.
pub struct FrameCache {
    pub scale: f32,
    /// Per-frame `[normal, flipped]` premultiplied BGRA buffers.
    pub entries: Vec<[Vec<u8>; 2]>,
    /// `(width, height)` in pixels of each scaled frame.
    pub dims: Vec<(u32, u32)>,
}

/// Build a `FrameCache` from a chromakey-applied spritesheet image and frame list.
///
/// Call this once after sheet load (chromakey already applied) and again whenever
/// the scale factor changes.  Rendering then becomes a plain buffer copy.
pub fn build_frame_cache(src: &RgbaImage, frames: &[Frame], scale: f32) -> FrameCache {
    let mut entries = Vec::with_capacity(frames.len());
    let mut dims = Vec::with_capacity(frames.len());
    for f in frames {
        let mut normal = Vec::new();
        blit_frame(src, f.x, f.y, f.w, f.h, &mut normal, scale, false);
        let mut flipped = Vec::new();
        blit_frame(src, f.x, f.y, f.w, f.h, &mut flipped, scale, true);
        let dw = (f.w as f32 * scale).round() as u32;
        let dh = (f.h as f32 * scale).round() as u32;
        dims.push((dw, dh));
        entries.push([normal, flipped]);
    }
    FrameCache { scale, entries, dims }
}

/// Returns the alpha value of the pixel at (`px`, `py`) in a premultiplied
/// BGRA buffer of given `width`.
#[allow(dead_code)]
pub fn alpha_at(buf: &[u8], width: u32, px: u32, py: u32) -> u8 {
    let idx = ((py * width + px) * 4 + 3) as usize;
    buf.get(idx).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn solid_rgba(r: u8, g: u8, b: u8, a: u8, w: u32, h: u32) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for p in img.pixels_mut() {
            *p = image::Rgba([r, g, b, a]);
        }
        img
    }

    #[test]
    fn premultiply_opaque() {
        let img = solid_rgba(200, 100, 50, 255, 4, 4);
        let mut dst = Vec::new();
        blit_frame(&img, 0, 0, 4, 4, &mut dst, 1.0, false);
        // B = 50, G = 100, R = 200, A = 255 (premultiplied unchanged for a=255)
        assert_eq!(dst[0], 50);
        assert_eq!(dst[1], 100);
        assert_eq!(dst[2], 200);
        assert_eq!(dst[3], 255);
    }

    #[test]
    fn premultiply_half_alpha() {
        let img = solid_rgba(200, 100, 50, 128, 1, 1);
        let mut dst = Vec::new();
        blit_frame(&img, 0, 0, 1, 1, &mut dst, 1.0, false);
        // pm(200, 128) = 200*128/255 ≈ 100
        assert!((dst[2] as i32 - 100).abs() <= 1);
        assert_eq!(dst[3], 128);
    }

    #[test]
    fn scale_doubles_size() {
        let img = solid_rgba(0, 255, 0, 255, 2, 2);
        let mut dst = Vec::new();
        blit_frame(&img, 0, 0, 2, 2, &mut dst, 2.0, false);
        assert_eq!(dst.len(), (4 * 4 * 4) as usize);
    }

    #[test]
    fn flip_horizontal() {
        // 2×1 image: left pixel red, right pixel blue
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 0, 255, 255]));
        let mut dst = Vec::new();
        blit_frame(&img, 0, 0, 2, 1, &mut dst, 1.0, true);
        // After flip: first pixel should be blue (B channel first in BGRA)
        assert_eq!(dst[0], 255); // B of blue
        assert_eq!(dst[2], 0);   // R of blue
        // second pixel should be red
        assert_eq!(dst[4], 0);   // B of red
        assert_eq!(dst[6], 255); // R of red
    }
}
