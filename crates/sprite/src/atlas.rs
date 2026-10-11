//! Packing many small images into a few large texture pages.

use std::fmt;

use crate::image::{Pixels, Rect};

/// Where one packed image went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub page: usize,
    pub rect: Rect,
}

/// An image too large for a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlasError {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub page_size: u32,
}

impl fmt::Display for AtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "image {} is {}x{}, too large for {size}x{size} atlas pages",
            self.index,
            self.width,
            self.height,
            size = self.page_size
        )
    }
}

impl std::error::Error for AtlasError {}

struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

#[derive(Default)]
struct Page {
    shelves: Vec<Shelf>,
    next_y: u32,
}

/// Places rectangles of the given sizes on square pages of `page_size`,
/// with `padding` empty pixels around each. Results are in input order and
/// depend only on the inputs.
pub fn pack(
    sizes: &[(u32, u32)],
    page_size: u32,
    padding: u32,
) -> Result<Vec<Placement>, AtlasError> {
    // tallest first packs shelves tightly; ties keep input order
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(sizes[i].1), std::cmp::Reverse(sizes[i].0)));

    let mut pages: Vec<Page> = Vec::new();
    let mut out = vec![
        Placement {
            page: 0,
            rect: Rect::default(),
        };
        sizes.len()
    ];
    for i in order {
        let (w, h) = sizes[i];
        let (pw, ph) = (w + padding, h + padding);
        if pw + padding > page_size || ph + padding > page_size {
            return Err(AtlasError {
                index: i,
                width: w,
                height: h,
                page_size,
            });
        }
        let spot = pages.iter_mut().enumerate().find_map(|(n, page)| {
            if let Some(shelf) = page
                .shelves
                .iter_mut()
                .find(|s| h <= s.height && s.next_x + pw <= page_size)
            {
                let x = shelf.next_x;
                shelf.next_x += pw;
                return Some((n, x, shelf.y));
            }
            let y = page.next_y.max(padding);
            (y + ph <= page_size).then(|| {
                page.shelves.push(Shelf {
                    y,
                    height: h,
                    next_x: padding + pw,
                });
                page.next_y = y + ph;
                (n, padding, y)
            })
        });
        let (page, x, y) = spot.unwrap_or_else(|| {
            pages.push(Page {
                shelves: vec![Shelf {
                    y: padding,
                    height: h,
                    next_x: padding + pw,
                }],
                next_y: padding + ph,
            });
            (pages.len() - 1, padding, padding)
        });
        out[i] = Placement {
            page,
            rect: Rect { x, y, w, h },
        };
    }
    Ok(out)
}

/// Images packed into pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Atlas<P> {
    pub pages: Vec<Pixels<P>>,
    /// One per image added, in the order they were added.
    pub entries: Vec<Placement>,
}

/// Collects images to pack into an [`Atlas`]. Works for RGBA images and
/// for index images alike.
#[derive(Clone, Debug)]
pub struct AtlasBuilder<P> {
    images: Vec<Pixels<P>>,
}

impl<P> Default for AtlasBuilder<P> {
    fn default() -> Self {
        AtlasBuilder { images: Vec::new() }
    }
}

impl<P: Copy + Default> AtlasBuilder<P> {
    /// Adds `rect` of `image`; returns its entry index in the atlas.
    pub fn add(&mut self, image: &Pixels<P>, rect: Rect) -> usize {
        self.images.push(image.crop(rect));
        self.images.len() - 1
    }

    pub fn build(&self, page_size: u32, padding: u32) -> Result<Atlas<P>, AtlasError> {
        let sizes: Vec<_> = self.images.iter().map(|i| (i.width, i.height)).collect();
        let entries = pack(&sizes, page_size, padding)?;
        let page_count = entries.iter().map(|e| e.page + 1).max().unwrap_or(0);
        let mut pages = vec![Pixels::new(page_size, page_size); page_count];
        for (image, entry) in self.images.iter().zip(&entries) {
            let whole = Rect {
                x: 0,
                y: 0,
                w: image.width,
                h: image.height,
            };
            pages[entry.page].blit(image, whole, entry.rect.x, entry.rect.y);
        }
        Ok(Atlas { pages, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: &Placement, b: &Placement, pad: u32) -> bool {
        a.page == b.page
            && a.rect.x < b.rect.x + b.rect.w + pad
            && b.rect.x < a.rect.x + a.rect.w + pad
            && a.rect.y < b.rect.y + b.rect.h + pad
            && b.rect.y < a.rect.y + a.rect.h + pad
    }

    #[test]
    fn packs_without_overlap_inside_pages() {
        let sizes: Vec<(u32, u32)> = (0..60).map(|i| (5 + i % 13, 4 + i % 7 * 3)).collect();
        let placed = pack(&sizes, 64, 1).unwrap();
        assert!(
            placed.iter().any(|p| p.page > 0),
            "should need several pages"
        );
        for (i, a) in placed.iter().enumerate() {
            assert_eq!((a.rect.w, a.rect.h), sizes[i]);
            assert!(a.rect.x >= 1 && a.rect.y >= 1);
            assert!(a.rect.x + a.rect.w < 64 && a.rect.y + a.rect.h < 64);
            for b in &placed[i + 1..] {
                assert!(!overlaps(a, b, 1), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn rejects_oversized() {
        assert_eq!(pack(&[(4, 4), (65, 2)], 64, 0).unwrap_err().index, 1);
        assert!(pack(&[(64, 64)], 64, 0).is_ok());
        assert!(pack(&[(63, 63)], 64, 1).is_err());
        assert!(pack(&[(62, 62)], 64, 1).is_ok());
    }
}
