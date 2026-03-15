use image::RgbaImage;

/// Copy a source frame region into `dst`, applying an integer upscale and
/// optional horizontal flip, while premultiplying alpha for Win32
/// `UpdateLayeredWindow` (which requires premultiplied BGRA).
///
/// `dst` must be exactly `w * scale` × `h * scale` pixels.
pub fn blit_frame(
    src: &RgbaImage,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
    dst: &mut Vec<u8>, // BGRA output for Win32 DIBSection
    scale: u32,
    flip_h: bool,
) {
    let dw = src_w * scale;
    let dh = src_h * scale;
    dst.resize((dw * dh * 4) as usize, 0);

    for dy in 0..dh {
        let sy = dy / scale;
        for dx in 0..dw {
            let raw_sx = dx / scale;
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

/// Returns the alpha value of the pixel at (`px`, `py`) in a premultiplied
/// BGRA buffer of given `width`.
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
        blit_frame(&img, 0, 0, 4, 4, &mut dst, 1, false);
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
        blit_frame(&img, 0, 0, 1, 1, &mut dst, 1, false);
        // pm(200, 128) = 200*128/255 ≈ 100
        assert!((dst[2] as i32 - 100).abs() <= 1);
        assert_eq!(dst[3], 128);
    }

    #[test]
    fn scale_doubles_size() {
        let img = solid_rgba(0, 255, 0, 255, 2, 2);
        let mut dst = Vec::new();
        blit_frame(&img, 0, 0, 2, 2, &mut dst, 2, false);
        assert_eq!(dst.len(), (4 * 4 * 4) as usize);
    }

    #[test]
    fn flip_horizontal() {
        // 2×1 image: left pixel red, right pixel blue
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 0, 255, 255]));
        let mut dst = Vec::new();
        blit_frame(&img, 0, 0, 2, 1, &mut dst, 1, true);
        // After flip: first pixel should be blue (B channel first in BGRA)
        assert_eq!(dst[0], 255); // B of blue
        assert_eq!(dst[2], 0);   // R of blue
        // second pixel should be red
        assert_eq!(dst[4], 0);   // B of red
        assert_eq!(dst[6], 255); // R of red
    }
}
