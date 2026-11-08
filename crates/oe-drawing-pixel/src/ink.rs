//! Binary ink of one page.

use anyhow::{Context, Result, bail, ensure};

/// Binary ink of one page: a row-major mask where `true` means "dark" (ink).
///
/// Pixel `(x, y)` is centred at `(x as f64, y as f64)`; queries outside the image read as not dark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ink {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    dark: Vec<bool>,
}

impl Ink {
    /// Decode PNG bytes and binarise them: a pixel is dark when its 8-bit grey value is `< threshold`.
    ///
    /// Any PNG colour type and bit depth is accepted (palette and low bit depths are expanded, 16-bit
    /// samples are reduced to 8). Colour is converted with the Rec. 601 luma weights
    /// `(299 R + 587 G + 114 B) / 1000`; alpha is composited over white first, so transparent areas are
    /// paper, not ink. This is the only I/O this crate performs.
    pub fn from_png(bytes: &[u8], threshold: u8) -> Result<Ink> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().context("png header")?;
        let size = reader.output_buffer_size().context("png output size")?;
        let mut buf = vec![0u8; size];
        let info = reader.next_frame(&mut buf).context("png pixels")?;
        ensure!(
            info.bit_depth == png::BitDepth::Eight,
            "png: bit depth {:?} after expansion is unsupported",
            info.bit_depth
        );
        let (width, height) = (info.width as usize, info.height as usize);
        ensure!(width > 0 && height > 0, "png: empty image");
        let channels = match info.color_type {
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            png::ColorType::Indexed => bail!("png: palette was not expanded"),
        };
        let stride = info.line_size;
        ensure!(
            stride >= width * channels && buf.len() >= stride * height,
            "png: decoded buffer shape mismatch"
        );
        let mut dark = Vec::with_capacity(width * height);
        let threshold = u32::from(threshold);
        for y in 0..height {
            let row = &buf[y * stride..y * stride + width * channels];
            for px in row.chunks_exact(channels) {
                let grey = match channels {
                    1 => u32::from(px[0]),
                    2 => over_white(u32::from(px[0]), u32::from(px[1])),
                    3 => luma(px[0], px[1], px[2]),
                    _ => over_white(luma(px[0], px[1], px[2]), u32::from(px[3])),
                };
                dark.push(grey < threshold);
            }
        }
        Ok(Ink {
            width,
            height,
            dark,
        })
    }

    /// Build ink from an explicit row-major mask (`dark[y * width + x]`), mainly for tests and for
    /// callers that binarise themselves.
    ///
    /// Panics when `dark.len() != width * height`.
    pub fn from_mask(width: usize, height: usize, dark: Vec<bool>) -> Ink {
        assert_eq!(
            dark.len(),
            width * height,
            "Ink::from_mask: mask length must equal width * height"
        );
        Ink {
            width,
            height,
            dark,
        }
    }

    /// Whether pixel `(x, y)` is dark; anything outside the image is not dark.
    #[inline]
    pub fn is_dark(&self, x: i64, y: i64) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.width
            && (y as usize) < self.height
            && self.dark[y as usize * self.width + x as usize]
    }

    /// Set pixel `(x, y)`; coordinates outside the image are ignored.
    #[inline]
    pub fn set_dark(&mut self, x: usize, y: usize, dark: bool) {
        if x < self.width && y < self.height {
            self.dark[y * self.width + x] = dark;
        }
    }

    /// Number of dark pixels.
    pub fn dark_count(&self) -> usize {
        self.dark.iter().filter(|&&d| d).count()
    }

    /// The row-major mask.
    pub fn mask(&self) -> &[bool] {
        &self.dark
    }

    /// Whether some dark pixel centre lies within Euclidean distance `r` of the point `(x, y)`.
    pub fn has_dark_within(&self, x: f64, y: f64, r: f64) -> bool {
        self.nearest_dark(x, y, r).is_some()
    }

    /// The dark pixel whose centre is nearest to `(x, y)` within radius `r`. Ties resolve to the
    /// first pixel in raster order.
    pub fn nearest_dark(&self, x: f64, y: f64, r: f64) -> Option<(usize, usize)> {
        if !(x.is_finite() && y.is_finite() && r.is_finite() && r >= 0.0) {
            return None;
        }
        let x0 = ((x - r).floor() as i64).max(0);
        let y0 = ((y - r).floor() as i64).max(0);
        let x1 = ((x + r).ceil() as i64).min(self.width as i64 - 1);
        let y1 = ((y + r).ceil() as i64).min(self.height as i64 - 1);
        let mut best: Option<(f64, usize, usize)> = None;
        let r2 = r * r + 1e-9;
        for yy in y0..=y1 {
            for xx in x0..=x1 {
                if !self.is_dark(xx, yy) {
                    continue;
                }
                let dx = xx as f64 - x;
                let dy = yy as f64 - y;
                let d2 = dx * dx + dy * dy;
                if d2 <= r2 && best.is_none_or(|(b, _, _)| d2 < b) {
                    best = Some((d2, xx as usize, yy as usize));
                }
            }
        }
        best.map(|(_, xx, yy)| (xx, yy))
    }
}

#[inline]
fn luma(r: u8, g: u8, b: u8) -> u32 {
    (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b) + 500) / 1000
}

/// Composite an 8-bit grey value with alpha over white paper.
#[inline]
fn over_white(grey: u32, alpha: u32) -> u32 {
    (grey * alpha + 255 * (255 - alpha) + 127) / 255
}
