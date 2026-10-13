//! The credits screen's text, from a content folder's `CREDITS.md`.
//!
//! The Markdown is read plainly: `##` headings become headings, list
//! items and paragraphs become lines, and emphasis, code marks and link
//! brackets are dropped (a link shows its address after its text). A `#`
//! heading is left out; the screen has its own.

use std::path::Path;

/// The credits shown when none are given, relative to the content folder.
pub const DEFAULT_CREDITS: &str = "base/CREDITS.md";

/// A line of credits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub heading: bool,
}

/// Credits as headings and paragraphs, one [`Line`] each, with empty
/// lines between.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Credits {
    pub lines: Vec<Line>,
}

impl Credits {
    pub fn load(path: &Path) -> Result<Credits, String> {
        std::fs::read_to_string(path)
            .map(|src| Credits::from_markdown(&src))
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn from_markdown(src: &str) -> Credits {
        let mut lines: Vec<Line> = Vec::new();
        // the paragraph or list item being read
        let mut open = false;
        let push = |lines: &mut Vec<Line>, text: String, heading: bool| {
            if heading && lines.last().is_some_and(|l| !l.text.is_empty()) {
                lines.push(Line {
                    text: String::new(),
                    heading: false,
                });
            }
            lines.push(Line { text, heading });
        };
        for raw in src.lines() {
            let line = raw.trim();
            if line.is_empty() {
                if open {
                    push(&mut lines, String::new(), false);
                }
                open = false;
            } else if line.starts_with("# ") {
                open = false;
            } else if let Some(heading) = line.strip_prefix("## ") {
                push(&mut lines, inline(heading), true);
                push(&mut lines, String::new(), false);
                open = false;
            } else if let Some(item) = line.strip_prefix("- ").or(line.strip_prefix("* ")) {
                push(&mut lines, format!("- {}", inline(item)), false);
                open = true;
            } else if open {
                let last = lines.last_mut().expect("an open paragraph has a line");
                last.text.push(' ');
                last.text.push_str(&inline(line));
            } else {
                push(&mut lines, inline(line), false);
                open = true;
            }
        }
        while lines.last().is_some_and(|l| l.text.is_empty()) {
            lines.pop();
        }
        Credits { lines }
    }

    /// The lines broken at spaces to at most `width` characters, or
    /// mid-word for a word longer than that. A list item's later lines
    /// are indented under its text.
    pub fn wrapped(&self, width: usize) -> Vec<Line> {
        let width = width.max(4);
        let mut out = Vec::new();
        for line in &self.lines {
            let indent = if line.text.starts_with("- ") { 2 } else { 0 };
            let mut rows = vec![String::new()];
            for word in line.text.split_whitespace() {
                let mut word: Vec<char> = word.chars().collect();
                loop {
                    let row = rows.last_mut().expect("there's always a row");
                    let used = row.chars().count();
                    let fresh = row.trim().is_empty();
                    let space = usize::from(!fresh);
                    if used + space + word.len() <= width {
                        if !fresh {
                            row.push(' ');
                        }
                        row.extend(&word);
                        break;
                    }
                    if fresh {
                        // longer than a line: break it
                        row.extend(word.drain(..width - used));
                    }
                    rows.push(" ".repeat(indent));
                }
            }
            if rows.len() > 1 && rows.last().is_some_and(|r| r.trim().is_empty()) {
                rows.pop();
            }
            out.extend(rows.into_iter().map(|text| Line {
                text,
                heading: line.heading,
            }));
        }
        out
    }
}

/// Text with Markdown's inline marks taken out: `**`, `*`, `_` and
/// backticks around words, and `[text](address)` as "text (address)".
fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let link = rest[open..].find("](").and_then(|mid| {
            let mid = open + mid;
            let close = mid + rest[mid..].find(')')?;
            Some((&rest[open + 1..mid], &rest[mid + 2..close], close))
        });
        let Some((label, address, close)) = link else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(label);
        if label != address {
            out.push_str(&format!(" ({address})"));
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out.replace("**", "")
        .replace('`', "")
        .split(' ')
        .map(|w| w.trim_matches(|c| c == '*' || c == '_'))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    const SAMPLE: &str = "\
# Credits

Third-party content. Everything
else is ours.

## Music

**A Pack** by **Someone**
- Source: https://music.test/pack
- License: [CC-BY 3.0](https://music.test/by/3.0/)
- Files: `music/a.mp3`,
  `music/b.mp3`
";

    #[test]
    fn reads_markdown_plainly() {
        let credits = Credits::from_markdown(SAMPLE);
        assert_eq!(
            texts(&credits.lines),
            [
                "Third-party content. Everything else is ours.",
                "",
                "Music",
                "",
                "A Pack by Someone",
                "- Source: https://music.test/pack",
                "- License: CC-BY 3.0 (https://music.test/by/3.0/)",
                "- Files: music/a.mp3, music/b.mp3",
            ]
        );
        let headings: Vec<_> = credits.lines.iter().map(|l| l.heading).collect();
        assert_eq!(headings.iter().filter(|&&h| h).count(), 1);
        assert!(credits.lines[2].heading);
    }

    #[test]
    fn wraps_at_spaces_and_indents_list_items() {
        let credits = Credits::from_markdown(SAMPLE);
        let wrapped = credits.wrapped(24);
        assert!(wrapped.iter().all(|l| l.text.chars().count() <= 24));
        assert_eq!(
            texts(&wrapped[..3]),
            ["Third-party content.", "Everything else is ours.", ""]
        );
        let files = wrapped
            .iter()
            .position(|l| l.text.starts_with("- Files"))
            .unwrap();
        assert_eq!(
            texts(&wrapped[files..]),
            ["- Files: music/a.mp3,", "  music/b.mp3"]
        );
        // an address longer than a line is broken
        let source = wrapped
            .iter()
            .position(|l| l.text.starts_with("- Source"))
            .unwrap();
        assert_eq!(
            texts(&wrapped[source..source + 3]),
            ["- Source:", "  https://music.test/pac", "  k"]
        );
    }

    #[test]
    fn the_bundled_credits_name_the_music() {
        let credits = Credits::load(&crate::content_dir().join(DEFAULT_CREDITS)).unwrap();
        let all = texts(&credits.lines).join("\n");
        assert!(
            all.contains("Platformer Game Music Pack by CodeManu"),
            "{all}"
        );
        assert!(all.contains("CC-BY 3.0 (https://creativecommons.org/licenses/by/3.0/)"));
        assert!(all.contains("https://opengameart.org/content/platformer-game-music-pack"));
    }
}
