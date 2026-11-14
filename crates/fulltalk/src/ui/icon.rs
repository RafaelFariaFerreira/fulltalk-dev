//! The app icon (a waveform on a violet squircle), rasterized at runtime so no
//! binary image needs to ship for the window/taskbar icon.

/// Relative heights of the five waveform bars.
pub const BARS: [f32; 5] = [0.28, 0.52, 0.78, 0.52, 0.28];

/// Renders the icon as straight RGBA8, `size × size`.
#[allow(clippy::many_single_char_names)] // geometry reads best with x/y/r/g/b
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let radius = s * 0.24;
    let bar_w = s * 0.085;
    let gap = s * 0.06;
    let total_w = bar_w * 5.0 + gap * 4.0;
    let x0 = (s - total_w) / 2.0;

    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let bg = coverage(rounded_rect_sdf(px, py, 0.0, 0.0, s, s, radius));

            let mut bar = 0.0f32;
            for (i, h) in BARS.iter().enumerate() {
                let bx = x0 + i as f32 * (bar_w + gap);
                let bh = s * h;
                let by = (s - bh) / 2.0;
                let d = rounded_rect_sdf(px, py, bx, by, bar_w, bh, bar_w / 2.0);
                bar = bar.max(coverage(d));
            }

            // Violet background, white bars on top.
            let (r, g, b) = lerp_rgb((0x5B, 0x3F, 0xE4), (0xFF, 0xFF, 0xFF), bar);
            out.extend_from_slice(&[r, g, b, (bg * 255.0).round() as u8]);
        }
    }
    out
}

pub fn icon_data(size: u32) -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: rgba(size),
        width: size,
        height: size,
    }
}

/// Signed distance from a point to a rounded rectangle (negative inside).
#[allow(clippy::many_single_char_names)]
fn rounded_rect_sdf(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let qx = (px - cx).abs() - (w / 2.0 - r);
    let qy = (py - cy).abs() - (h / 2.0 - r);
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    outside + qx.max(qy).min(0.0) - r
}

/// Anti-aliased coverage for a signed distance (1px feather).
fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(buf: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn has_expected_byte_length() {
        assert_eq!(rgba(64).len(), 64 * 64 * 4);
    }

    #[test]
    fn corners_are_transparent_and_center_bar_is_white() {
        let size = 128;
        let img = rgba(size);
        assert_eq!(pixel(&img, size, 0, 0)[3], 0, "rounded corner");
        assert_eq!(pixel(&img, size, size / 2, size / 2), [255, 255, 255, 255]);
    }

    #[test]
    fn background_between_bars_is_violet() {
        let size = 128;
        let img = rgba(size);
        // Near the top edge, above every bar.
        assert_eq!(pixel(&img, size, size / 2, 8), [0x5B, 0x3F, 0xE4, 255]);
    }

    #[test]
    fn sdf_is_negative_inside_and_positive_outside() {
        assert!(rounded_rect_sdf(5.0, 5.0, 0.0, 0.0, 10.0, 10.0, 2.0) < 0.0);
        assert!(rounded_rect_sdf(20.0, 5.0, 0.0, 0.0, 10.0, 10.0, 2.0) > 0.0);
    }
}
