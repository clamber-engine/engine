//! Pixel buffers, palettes and PNG decoding.

use std::fmt;
use std::io::Cursor;

/// An RGBA color, 8 bits per channel, not premultiplied.
pub type Rgba = [u8; 4];

/// A rectangle of pixels, row by row from the top left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixels<P> {
    pub width: u32,
    pub height: u32,
    pub data: Vec<P>,
}

/// A full-color image.
pub type RgbaImage = Pixels<Rgba>;
/// Palette indices, for palette swaps.
pub type IndexImage = Pixels<u8>;

/// A rectangle in an image, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    /// Whether the rectangle lies inside a `width` × `height` image.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        self.x.checked_add(self.w).is_some_and(|r| r <= width)
            && self.y.checked_add(self.h).is_some_and(|b| b <= height)
    }
}

impl<P: Copy + Default> Pixels<P> {
    /// An image filled with `P::default()` (transparent, or index 0).
    pub fn new(width: u32, height: u32) -> Self {
        Pixels {
            width,
            height,
            data: vec![P::default(); width as usize * height as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> P {
        self.data[self.index(x, y)]
    }

    pub fn set(&mut self, x: u32, y: u32, value: P) {
        let i = self.index(x, y);
        self.data[i] = value;
    }

    fn index(&self, x: u32, y: u32) -> usize {
        assert!(x < self.width && y < self.height, "pixel out of bounds");
        y as usize * self.width as usize + x as usize
    }

    /// Copies `rect` of `src` to `(x, y)` in this image.
    pub fn blit(&mut self, src: &Pixels<P>, rect: Rect, x: u32, y: u32) {
        assert!(
            rect.fits(src.width, src.height),
            "source rect out of bounds"
        );
        assert!(
            Rect { x, y, ..rect }.fits(self.width, self.height),
            "destination out of bounds"
        );
        for row in 0..rect.h {
            let s = src.index(rect.x, rect.y + row);
            let d = self.index(x, y + row);
            let w = rect.w as usize;
            self.data[d..d + w].copy_from_slice(&src.data[s..s + w]);
        }
    }

    /// A copy of `rect` as its own image.
    pub fn crop(&self, rect: Rect) -> Pixels<P> {
        let mut out = Pixels::new(rect.w, rect.h);
        out.blit(self, rect, 0, 0);
        out
    }
}

/// An ordered list of colors for indexed images.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Palette(pub Vec<Rgba>);

impl Palette {
    /// Colors an index image. Indices past the end of the palette are
    /// transparent.
    pub fn apply(&self, indices: &IndexImage) -> RgbaImage {
        Pixels {
            width: indices.width,
            height: indices.height,
            data: indices
                .data
                .iter()
                .map(|&i| self.0.get(i as usize).copied().unwrap_or_default())
                .collect(),
        }
    }

    /// This palette with every visible color replaced by `color`, keeping
    /// each entry's alpha. Used for hit flashes and silhouettes.
    pub fn flash(&self, color: Rgba) -> Palette {
        Palette(
            self.0
                .iter()
                .map(|c| [color[0], color[1], color[2], c[3]])
                .collect(),
        )
    }

    /// Parses a color written `#rrggbb` or `#rrggbbaa`.
    pub fn parse_color(s: &str) -> Option<Rgba> {
        let hex = s.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
        Some([byte(0)?, byte(2)?, byte(4)?, alpha])
    }
}

/// A decoded image: full color, or palette indices with the file's palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Image {
    Rgba(RgbaImage),
    Indexed {
        indices: IndexImage,
        palette: Palette,
    },
}

impl Image {
    pub fn width(&self) -> u32 {
        match self {
            Image::Rgba(p) => p.width,
            Image::Indexed { indices, .. } => indices.width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Image::Rgba(p) => p.height,
            Image::Indexed { indices, .. } => indices.height,
        }
    }

    /// The image in full color, using its own palette if it has one.
    pub fn to_rgba(&self) -> RgbaImage {
        match self {
            Image::Rgba(p) => p.clone(),
            Image::Indexed { indices, palette } => palette.apply(indices),
        }
    }
}

/// An image that couldn't be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageError(pub String);

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImageError {}

impl From<png::DecodingError> for ImageError {
    fn from(e: png::DecodingError) -> Self {
        ImageError(format!("invalid PNG: {e}"))
    }
}

/// Decodes a PNG. Indexed PNGs stay indexed, so their palettes can be
/// swapped; everything else becomes RGBA.
pub fn decode_png(bytes: &[u8]) -> Result<Image, ImageError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    let indexed = decoder.read_header_info()?.color_type == png::ColorType::Indexed;
    decoder.set_transformations(if indexed {
        png::Transformations::IDENTITY
    } else {
        png::Transformations::normalize_to_color8()
    });
    let mut reader = decoder.read_info()?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| ImageError("PNG too large".into()))?;
    let mut buf = vec![0; size];
    let frame = reader.next_frame(&mut buf)?;
    let (width, height) = (frame.width, frame.height);
    let (color, depth) = reader.output_color_type();
    let line = frame.line_size;

    if indexed {
        let info = reader.info();
        let rgb = info.palette.as_deref().unwrap_or_default();
        let alpha = info.trns.as_deref().unwrap_or_default();
        let palette = Palette(
            rgb.as_chunks::<3>()
                .0
                .iter()
                .enumerate()
                .map(|(i, &[r, g, b])| [r, g, b, alpha.get(i).copied().unwrap_or(255)])
                .collect(),
        );
        let bits = depth as usize;
        let mut indices = Pixels::new(width, height);
        for y in 0..height as usize {
            let row = &buf[y * line..(y + 1) * line];
            for x in 0..width as usize {
                let bit = x * bits;
                let shift = 8 - bits - bit % 8;
                let index = (row[bit / 8] >> shift) & ((1u16 << bits) - 1) as u8;
                indices.data[y * width as usize + x] = index;
            }
        }
        return Ok(Image::Indexed { indices, palette });
    }

    let channels = color.samples();
    let mut out = Pixels::new(width, height);
    for y in 0..height as usize {
        let row = &buf[y * line..(y + 1) * line];
        for (x, px) in row.chunks_exact(channels).take(width as usize).enumerate() {
            out.data[y * width as usize + x] = match *px {
                [v] => [v, v, v, 255],
                [v, a] => [v, v, v, a],
                [r, g, b] => [r, g, b, 255],
                [r, g, b, a] => [r, g, b, a],
                _ => unreachable!("PNG samples are 1 to 4 channels"),
            };
        }
    }
    Ok(Image::Rgba(out))
}

/// Encodes an RGBA image as PNG.
pub fn encode_png(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header");
    writer
        .write_image_data(image.data.as_flattened())
        .expect("PNG data");
    writer.finish().expect("PNG end");
    out
}

/// Encodes palette indices as an 8-bit indexed PNG, with `palette`'s
/// alpha as its transparency. `palette` has at most 256 colors.
pub fn encode_indexed_png(indices: &IndexImage, palette: &Palette) -> Vec<u8> {
    assert!(
        palette.0.len() <= 256,
        "a PNG palette has at most 256 colors"
    );
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, indices.width, indices.height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(
        palette
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2]])
            .collect::<Vec<_>>(),
    );
    // Trailing opaque entries can be left out of the transparency chunk.
    let alpha: Vec<u8> = palette.0.iter().map(|c| c[3]).collect();
    let used = alpha.iter().rposition(|&a| a != 255).map_or(0, |i| i + 1);
    if used > 0 {
        encoder.set_trns(alpha[..used].to_vec());
    }
    let mut writer = encoder.write_header().expect("PNG header");
    writer.write_image_data(&indices.data).expect("PNG data");
    writer.finish().expect("PNG end");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(Palette::parse_color("#ff8000"), Some([255, 128, 0, 255]));
        assert_eq!(Palette::parse_color("#ff800080"), Some([255, 128, 0, 128]));
        assert_eq!(Palette::parse_color("ff8000"), None);
        assert_eq!(Palette::parse_color("#ff80"), None);
        assert_eq!(Palette::parse_color("#gg8000"), None);
    }

    #[test]
    fn blit_and_crop() {
        let mut a: IndexImage = Pixels::new(4, 3);
        a.set(1, 1, 5);
        a.set(2, 2, 6);
        let c = a.crop(Rect {
            x: 1,
            y: 1,
            w: 2,
            h: 2,
        });
        assert_eq!(c.data, [5, 0, 0, 6]);
    }

    #[test]
    fn rgba_round_trip() {
        let mut img: RgbaImage = Pixels::new(3, 2);
        img.set(2, 1, [1, 2, 3, 4]);
        let Image::Rgba(back) = decode_png(&encode_png(&img)).unwrap() else {
            panic!("expected RGBA");
        };
        assert_eq!(back, img);
    }

    #[test]
    fn indexed_round_trip() {
        let mut indices: IndexImage = Pixels::new(3, 2);
        indices.set(0, 0, 1);
        indices.set(2, 1, 2);
        let palette = Palette(vec![[0, 0, 0, 0], [10, 20, 30, 255], [40, 50, 60, 128]]);
        let back = decode_png(&encode_indexed_png(&indices, &palette)).unwrap();
        assert_eq!(back, Image::Indexed { indices, palette });
    }
}
