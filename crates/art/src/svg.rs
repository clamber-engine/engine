//! SVG rendering through resvg.
//!
//! Placeholder art is pixel art, so sources normally set
//! `shape-rendering="crispEdges"` on the root: edges are then not
//! anti-aliased and every pixel takes one of the SVG's own colors, which
//! is what lets sheets be stored indexed. Text isn't supported; draw it as
//! shapes.

use clamber_sprite::{Pixels, RgbaImage};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

/// Renders an SVG at its own size, one pixel per user unit.
pub fn render(src: &str) -> Result<RgbaImage, String> {
    let tree = Tree::from_str(src, &Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height())
        .ok_or_else(|| format!("can't render at {}x{}", size.width(), size.height()))?;
    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
    let data = pixmap
        .pixels()
        .iter()
        .map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Ok(Pixels {
        width: size.width(),
        height: size.height(),
        data,
    })
}
