//! A small bitmap font defined in code, drawn at whole-pixel positions
//! with a drop shadow so it reads over any background.
//!
//! Glyphs are 5 by 7 pixels. Letters are capitals only; lower case draws
//! as upper case, and characters without a glyph draw as `?`. A few
//! symbols stand for HUD icons: `♥` (lives), `◆` (gems) and `★` (relics).
//! The font is a placeholder, like the rest of the art for now.

use crate::canvas::{Color, rgb};
use crate::surface::Surface;

/// A glyph's size in pixels.
pub const GLYPH: (i32, i32) = (5, 7);
/// From one character's left edge to the next.
pub const ADVANCE: i32 = 6;
/// From one line's top to the next.
pub const LINE: i32 = 9;

/// The shadow under text, one pixel down and right.
pub const SHADOW: Color = rgb(0x10, 0x10, 0x18);

#[rustfmt::skip]
const GLYPHS: &[(char, [&str; 7])] = &[
    (' ', ["     ", "     ", "     ", "     ", "     ", "     ", "     "]),
    ('0', [" ### ", "#   #", "#  ##", "# # #", "##  #", "#   #", " ### "]),
    ('1', ["  #  ", " ##  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### "]),
    ('2', [" ### ", "#   #", "    #", "   # ", "  #  ", " #   ", "#####"]),
    ('3', ["#####", "   # ", "  #  ", "   # ", "    #", "#   #", " ### "]),
    ('4', ["   # ", "  ## ", " # # ", "#  # ", "#####", "   # ", "   # "]),
    ('5', ["#####", "#    ", "#### ", "    #", "    #", "#   #", " ### "]),
    ('6', ["  ## ", " #   ", "#    ", "#### ", "#   #", "#   #", " ### "]),
    ('7', ["#####", "    #", "   # ", "  #  ", " #   ", " #   ", " #   "]),
    ('8', [" ### ", "#   #", "#   #", " ### ", "#   #", "#   #", " ### "]),
    ('9', [" ### ", "#   #", "#   #", " ####", "    #", "   # ", " ##  "]),
    ('A', [" ### ", "#   #", "#   #", "#####", "#   #", "#   #", "#   #"]),
    ('B', ["#### ", "#   #", "#   #", "#### ", "#   #", "#   #", "#### "]),
    ('C', [" ### ", "#   #", "#    ", "#    ", "#    ", "#   #", " ### "]),
    ('D', ["#### ", "#   #", "#   #", "#   #", "#   #", "#   #", "#### "]),
    ('E', ["#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####"]),
    ('F', ["#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#    "]),
    ('G', [" ### ", "#   #", "#    ", "# ###", "#   #", "#   #", " ####"]),
    ('H', ["#   #", "#   #", "#   #", "#####", "#   #", "#   #", "#   #"]),
    ('I', [" ### ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### "]),
    ('J', ["  ###", "   # ", "   # ", "   # ", "   # ", "#  # ", " ##  "]),
    ('K', ["#   #", "#  # ", "# #  ", "##   ", "# #  ", "#  # ", "#   #"]),
    ('L', ["#    ", "#    ", "#    ", "#    ", "#    ", "#    ", "#####"]),
    ('M', ["#   #", "## ##", "# # #", "# # #", "#   #", "#   #", "#   #"]),
    ('N', ["#   #", "#   #", "##  #", "# # #", "#  ##", "#   #", "#   #"]),
    ('O', [" ### ", "#   #", "#   #", "#   #", "#   #", "#   #", " ### "]),
    ('P', ["#### ", "#   #", "#   #", "#### ", "#    ", "#    ", "#    "]),
    ('Q', [" ### ", "#   #", "#   #", "#   #", "# # #", "#  # ", " ## #"]),
    ('R', ["#### ", "#   #", "#   #", "#### ", "# #  ", "#  # ", "#   #"]),
    ('S', [" ####", "#    ", "#    ", " ### ", "    #", "    #", "#### "]),
    ('T', ["#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  "]),
    ('U', ["#   #", "#   #", "#   #", "#   #", "#   #", "#   #", " ### "]),
    ('V', ["#   #", "#   #", "#   #", "#   #", "#   #", " # # ", "  #  "]),
    ('W', ["#   #", "#   #", "#   #", "# # #", "# # #", "# # #", " # # "]),
    ('X', ["#   #", "#   #", " # # ", "  #  ", " # # ", "#   #", "#   #"]),
    ('Y', ["#   #", "#   #", " # # ", "  #  ", "  #  ", "  #  ", "  #  "]),
    ('Z', ["#####", "    #", "   # ", "  #  ", " #   ", "#    ", "#####"]),
    ('.', ["     ", "     ", "     ", "     ", "     ", " ##  ", " ##  "]),
    (',', ["     ", "     ", "     ", "     ", " ##  ", " ##  ", "  #  "]),
    (':', ["     ", " ##  ", " ##  ", "     ", " ##  ", " ##  ", "     "]),
    (';', ["     ", " ##  ", " ##  ", "     ", " ##  ", " ##  ", "  #  "]),
    ('!', ["  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "     ", "  #  "]),
    ('?', [" ### ", "#   #", "    #", "   # ", "  #  ", "     ", "  #  "]),
    ('\'', ["  #  ", "  #  ", "     ", "     ", "     ", "     ", "     "]),
    ('…', ["     ", "     ", "     ", "     ", "     ", "     ", "# # #"]),
    ('-', ["     ", "     ", "     ", " ### ", "     ", "     ", "     "]),
    ('_', ["     ", "     ", "     ", "     ", "     ", "     ", "#####"]),
    ('+', ["     ", "  #  ", "  #  ", "#####", "  #  ", "  #  ", "     "]),
    ('%', ["##   ", "##  #", "   # ", "  #  ", " #   ", "#  ##", "   ##"]),
    ('/', ["    #", "    #", "   # ", "  #  ", " #   ", "#    ", "#    "]),
    ('(', ["   # ", "  #  ", " #   ", " #   ", " #   ", "  #  ", "   # "]),
    (')', [" #   ", "  #  ", "   # ", "   # ", "   # ", "  #  ", " #   "]),
    ('<', ["   # ", "  #  ", " #   ", "#    ", " #   ", "  #  ", "   # "]),
    ('>', [" #   ", "  #  ", "   # ", "    #", "   # ", "  #  ", " #   "]),
    ('♥', ["     ", "## ##", "#####", "#####", " ### ", "  #  ", "     "]),
    ('◆', ["  #  ", " ### ", "#####", "#####", " ### ", "  #  ", "     "]),
    ('★', ["  #  ", "  #  ", "#####", " ### ", " ### ", "## ##", "#   #"]),
];

fn glyph(c: char) -> &'static [&'static str; 7] {
    let c = c.to_ascii_uppercase();
    let find = |c| GLYPHS.iter().find(|g| g.0 == c).map(|g| &g.1);
    find(c).or_else(|| find('?')).expect("the font has '?'")
}

/// The size of `text` drawn without its shadow: the widest line, and
/// every line.
pub fn text_size(text: &str) -> (i32, i32) {
    let width = text
        .lines()
        .map(|line| line.chars().count() as i32 * ADVANCE - 1)
        .max()
        .unwrap_or(0)
        .max(0);
    let lines = text.lines().count() as i32;
    let height = if lines == 0 {
        0
    } else {
        (lines - 1) * LINE + GLYPH.1
    };
    (width, height)
}

/// `text` in lines of at most `width` characters, broken between words
/// where it can be.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        while !word.is_empty() {
            let used = line.chars().count();
            let space = usize::from(used > 0);
            if used + space + word.len() <= width {
                if space > 0 {
                    line.push(' ');
                }
                line.extend(word.drain(..));
            } else if used == 0 {
                // Longer than a line: break it.
                line.extend(word.drain(..width));
                lines.push(std::mem::take(&mut line));
            } else {
                lines.push(std::mem::take(&mut line));
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Draws `text` with its top left at `(x, y)` and its shadow under it.
/// `\n` starts a new line.
pub fn draw_text(canvas: &mut dyn Surface, x: i32, y: i32, text: &str, color: Color) {
    draw_plain(canvas, x + 1, y + 1, text, SHADOW);
    draw_plain(canvas, x, y, text, color);
}

/// Draws `text` centred on `centre_x`, its top at `y`.
pub fn draw_text_centred(
    canvas: &mut dyn Surface,
    centre_x: i32,
    y: i32,
    text: &str,
    color: Color,
) {
    draw_text(canvas, centre_x - text_size(text).0 / 2, y, text, color);
}

fn draw_plain(canvas: &mut dyn Surface, x: i32, y: i32, text: &str, color: Color) {
    for (row, line) in text.lines().enumerate() {
        let top = y + row as i32 * LINE;
        for (col, c) in line.chars().enumerate() {
            let left = x + col as i32 * ADVANCE;
            for (dy, bits) in glyph(c).iter().enumerate() {
                for (dx, _) in bits.bytes().enumerate().filter(|b| b.1 == b'#') {
                    canvas.plot(left + dx as i32, top + dy as i32, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;

    const WHITE: Color = rgb(255, 255, 255);

    /// The canvas as rows of `#` for `color`, `s` for shadow, `.` else.
    fn picture(canvas: &Canvas) -> Vec<String> {
        canvas
            .pixels
            .chunks(canvas.width as usize)
            .map(|row| {
                row.iter()
                    .map(|&p| match p {
                        WHITE => '#',
                        SHADOW => 's',
                        _ => '.',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn every_glyph_is_five_by_seven_and_unique() {
        for (c, rows) in GLYPHS {
            assert!(rows.iter().all(|r| r.len() == 5), "{c:?}");
            assert!(
                rows.iter()
                    .all(|r| r.bytes().all(|b| b == b' ' || b == b'#'))
            );
        }
        for (i, a) in GLYPHS.iter().enumerate() {
            assert!(GLYPHS[i + 1..].iter().all(|b| b.0 != a.0), "{:?}", a.0);
        }
    }

    #[test]
    fn draws_with_a_shadow() {
        let mut canvas = Canvas::new(8, 9);
        draw_text(&mut canvas, 1, 1, "1", WHITE);
        assert_eq!(
            picture(&canvas),
            [
                "........", "...#....", "..##s...", "...#s...", "...#s...", "...#s...", "...#s...",
                "..###...", "...sss..",
            ]
        );
    }

    #[test]
    fn upper_cases_and_falls_back_to_a_question_mark() {
        assert_eq!(glyph('a'), glyph('A'));
        assert_eq!(glyph('~'), glyph('?'));
        assert_eq!(glyph('é'), glyph('?'));
    }

    #[test]
    fn measures_lines() {
        assert_eq!(text_size(""), (0, 0));
        assert_eq!(text_size("A"), (5, 7));
        assert_eq!(text_size("AB"), (11, 7));
        assert_eq!(text_size("AB\nCDE"), (17, 16));
    }

    #[test]
    fn wraps_between_words() {
        assert_eq!(wrap("", 10), Vec::<String>::new());
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("  spaced   out ", 20), ["spaced out"]);
        assert_eq!(wrap("abcdefghij k", 4), ["abcd", "efgh", "ij k"]);
    }

    #[test]
    fn clips_at_the_edges() {
        let mut canvas = Canvas::new(4, 4);
        draw_text(&mut canvas, -3, -3, "W\nW", WHITE);
        draw_text_centred(&mut canvas, 2, 2, "WWWW", WHITE);
        assert!(canvas.pixels.contains(&WHITE));
    }
}
