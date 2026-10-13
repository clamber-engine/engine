//! Headless capture: plays an input script through a [`Session`]
//! offscreen and keeps frames for a gameplay GIF and a few screenshots
//! of its liveliest moments. Nothing here needs a window or a GPU.

use std::collections::HashMap;

use clamber_sim::Script;

use crate::VIEW;
use crate::canvas::{Canvas, Color};
use crate::session::Session;
use crate::sound::Cue;

/// GIF frame delay, in hundredths of a second: 25 frames a second.
pub const GIF_DELAY: u16 = 4;

/// Logic steps per kept frame, as a fraction: 12 steps make 5 frames,
/// so 60 Hz play comes out at 25 frames a second, in real time.
const STEPS: u32 = 12;
const KEPT: u32 = 5;

/// Screenshots taken from a capture, at most.
pub const SHOTS: usize = 4;

/// Frames kept from a scripted run, and the ones worth a screenshot.
pub struct Capture {
    /// One every [`GIF_DELAY`] hundredths of a second of play.
    pub frames: Vec<Canvas>,
    /// Indices into `frames`, in order.
    pub shots: Vec<usize>,
}

/// Plays `script` from wherever `session` is (usually its title
/// screen), drawing a frame every [`GIF_DELAY`] hundredths of a second
/// at `scale` pixels per view unit.
pub fn capture(session: &mut Session, script: &Script, scale: u32) -> Capture {
    let mut frames = Vec::new();
    // Where the run starts, usually the title screen, is worth a shot.
    let mut moments = vec![(2, 0)];
    let mut canvas = Canvas::scaled(VIEW, scale);
    for (step, input) in (0u32..).zip(script.frames()) {
        session.step(&input);
        for &cue in &session.cues {
            if let Some((rank, delay)) = interest(cue) {
                moments.push((rank, step + delay));
            }
        }
        if is_kept(step) {
            session.draw(&mut canvas, 1.0);
            frames.push(canvas.clone());
        }
    }
    let kept = |step: u32| (step * KEPT).div_ceil(STEPS) as usize;
    let moments: Vec<(u8, usize)> = moments
        .into_iter()
        .map(|(rank, step)| (rank, kept(step)))
        .filter(|&(_, frame)| frame < frames.len())
        .collect();
    let shots = pick_shots(&moments, frames.len(), SHOTS);
    Capture { frames, shots }
}

/// Plays `script` again, as [`capture`] did from the same starting
/// `session`, and draws just the kept frames numbered in `wanted` (as in
/// [`Capture::frames`]) at `scale` pixels per view unit: screenshots at a
/// higher resolution than the GIF without keeping every frame that large.
/// Play is deterministic, so they show what the capture's frames did.
pub fn redraw(session: &mut Session, script: &Script, wanted: &[usize], scale: u32) -> Vec<Canvas> {
    let mut drawn = Vec::new();
    let mut kept = 0;
    for (step, input) in (0u32..).zip(script.frames()) {
        if drawn.len() == wanted.len() {
            break;
        }
        session.step(&input);
        if is_kept(step) {
            if wanted.contains(&kept) {
                let mut canvas = Canvas::scaled(VIEW, scale);
                session.draw(&mut canvas, 1.0);
                drawn.push(canvas);
            }
            kept += 1;
        }
    }
    drawn
}

/// Whether the frame after logic step `step` is kept.
fn is_kept(step: u32) -> bool {
    step * KEPT % STEPS < KEPT
}

/// How worth showing what follows a cue is, and how many steps after it
/// to look: mid-air after a jump, the menu after pausing or the screen
/// after the level ends.
fn interest(cue: Cue) -> Option<(u8, u32)> {
    match cue {
        Cue::Complete | Cue::GameOver => Some((3, 40)),
        Cue::Collect | Cue::Checkpoint | Cue::Hurt | Cue::Defeat => Some((2, 4)),
        Cue::Launch => Some((2, 16)),
        Cue::Pause => Some((2, 8)),
        Cue::Jump | Cue::Roll => Some((1, 16)),
        Cue::Land => None,
    }
}

/// Up to `count` frames out of `len`: the highest-ranked `moments`
/// first, kept apart so they show different things, then evenly spaced
/// frames to fill in. In order.
fn pick_shots(moments: &[(u8, usize)], len: usize, count: usize) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let apart = len / (2 * count);
    let mut ranked = moments.to_vec();
    // Highest rank first; among equals, earliest first.
    ranked.sort_by_key(|&(rank, frame)| (std::cmp::Reverse(rank), frame));
    let even = (0..count).map(|i| (0, (2 * i + 1) * len / (2 * count)));

    let mut shots: Vec<usize> = Vec::new();
    for (_, frame) in ranked.into_iter().chain(even) {
        if shots.len() == count {
            break;
        }
        if shots.iter().all(|&s| s.abs_diff(frame) >= apart.max(1)) {
            shots.push(frame);
        }
    }
    shots.sort_unstable();
    shots
}

/// `canvas` scaled up by `scale` as 8-bit RGB PNG bytes.
pub fn encode_png(canvas: &Canvas, scale: u32) -> Result<Vec<u8>, String> {
    let (width, height) = (canvas.width * scale, canvas.height * scale);
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let c = canvas.pixels[((y / scale) * canvas.width + x / scale) as usize];
            rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Palette entries for colours; one more is kept for "unchanged".
const COLORS: usize = 255;
const UNCHANGED: u8 = 255;

/// `frames` scaled up by `scale` as a looping GIF, `delay` hundredths of
/// a second apart.
/// All frames share one palette of the colours they use, reduced to 255
/// when there are more. After the first, each frame only stores the
/// rectangle that changed, with unchanged pixels in it transparent.
pub fn encode_gif(frames: &[Canvas], scale: u32, delay: u16) -> Result<Vec<u8>, String> {
    let Some(first) = frames.first() else {
        return Err("no frames to encode".into());
    };
    let (width, height) = (first.width, first.height);
    let to_u16 = |n: u32| u16::try_from(n).map_err(|_| "frames too large for a GIF".to_string());
    let palette = Palette::new(frames);

    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(
            &mut out,
            to_u16(width * scale)?,
            to_u16(height * scale)?,
            &palette.bytes(),
        )
        .map_err(|e| e.to_string())?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;

        let mut previous: Option<Vec<u8>> = None;
        for canvas in frames {
            if (canvas.width, canvas.height) != (width, height) {
                return Err("frames differ in size".into());
            }
            let indices: Vec<u8> = canvas.pixels.iter().map(|&c| palette.index(c)).collect();
            let (x0, y0, x1, y1) = match &previous {
                None => (0, 0, width, height),
                Some(prev) => changed(prev, &indices, width).unwrap_or((0, 0, 1, 1)),
            };
            let (w, h) = ((x1 - x0) * scale, (y1 - y0) * scale);
            let mut buffer = Vec::with_capacity((w * h) as usize);
            for y in 0..h {
                let row = ((y0 + y / scale) * width) as usize;
                for x in 0..w {
                    let i = row + (x0 + x / scale) as usize;
                    let unchanged = previous.as_ref().is_some_and(|p| p[i] == indices[i]);
                    buffer.push(if unchanged { UNCHANGED } else { indices[i] });
                }
            }
            let frame = gif::Frame {
                left: to_u16(x0 * scale)?,
                top: to_u16(y0 * scale)?,
                width: to_u16(w)?,
                height: to_u16(h)?,
                delay,
                dispose: gif::DisposalMethod::Keep,
                transparent: previous.is_some().then_some(UNCHANGED),
                buffer: buffer.into(),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame).map_err(|e| e.to_string())?;
            previous = Some(indices);
        }
    }
    Ok(out)
}

/// The smallest rectangle, `(x0, y0, x1, y1)` exclusive, holding every
/// pixel that differs between two frames `width` wide; `None` if none do.
fn changed(a: &[u8], b: &[u8], width: u32) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (i, _) in a.iter().zip(b).enumerate().filter(|(_, (p, q))| p != q) {
        let (x, y) = (i as u32 % width, i as u32 / width);
        bounds = Some(match bounds {
            None => (x, y, x + 1, y + 1),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
        });
    }
    bounds
}

/// Up to [`COLORS`] colours for a set of frames, and which entry stands
/// for each colour in them.
struct Palette {
    colors: Vec<Color>,
    index: HashMap<Color, u8>,
}

impl Palette {
    fn new(frames: &[Canvas]) -> Palette {
        let mut counts: HashMap<Color, u32> = HashMap::new();
        for canvas in frames {
            for &c in &canvas.pixels {
                *counts.entry(c).or_default() += 1;
            }
        }
        let mut used: Vec<(Color, u32)> = counts.into_iter().collect();
        used.sort_unstable();
        let colors = if used.len() <= COLORS {
            used.iter().map(|&(c, _)| c).collect()
        } else {
            median_cut(used.clone(), COLORS)
        };
        let index = used
            .iter()
            .map(|&(c, _)| (c, nearest(&colors, c)))
            .collect();
        Palette { colors, index }
    }

    fn index(&self, color: Color) -> u8 {
        self.index[&color]
    }

    /// The 256-entry RGB table; unused entries are black.
    fn bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0; 256 * 3];
        for (entry, &c) in bytes.chunks_mut(3).zip(&self.colors) {
            entry.copy_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        }
        bytes
    }
}

fn channels(c: Color) -> [i32; 3] {
    [
        (c >> 16 & 0xff) as i32,
        (c >> 8 & 0xff) as i32,
        (c & 0xff) as i32,
    ]
}

/// The entry in `colors` closest to `color`.
fn nearest(colors: &[Color], color: Color) -> u8 {
    let [r, g, b] = channels(color);
    let distance = |&c: &Color| {
        let [r2, g2, b2] = channels(c);
        (r - r2).pow(2) + (g - g2).pow(2) + (b - b2).pow(2)
    };
    (0..colors.len())
        .min_by_key(|&i| distance(&colors[i]))
        .unwrap_or(0) as u8
}

/// Reduces counted colours to `count` by repeatedly splitting the box
/// with the widest channel range at its pixel-weighted median, then
/// averaging each box.
fn median_cut(colors: Vec<(Color, u32)>, count: usize) -> Vec<Color> {
    let range = |b: &[(Color, u32)]| -> (i32, usize) {
        (0..3)
            .map(|ch| {
                let values = b.iter().map(|&(c, _)| channels(c)[ch]);
                let span = values.clone().max().unwrap_or(0) - values.min().unwrap_or(0);
                (span, ch)
            })
            .max()
            .unwrap_or((0, 0))
    };
    let mut boxes = vec![colors];
    while boxes.len() < count {
        let Some((widest, _)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .max_by_key(|(_, b)| range(b).0)
        else {
            break;
        };
        let mut b = boxes.swap_remove(widest);
        let ch = range(&b).1;
        b.sort_unstable_by_key(|&(c, _)| channels(c)[ch]);
        let total: u64 = b.iter().map(|&(_, n)| u64::from(n)).sum();
        let mut seen = 0;
        let split = b
            .iter()
            .position(|&(_, n)| {
                seen += u64::from(n);
                seen * 2 >= total
            })
            .map_or(1, |i| i.clamp(1, b.len() - 1));
        let rest = b.split_off(split);
        boxes.push(b);
        boxes.push(rest);
    }
    boxes
        .iter()
        .map(|b| {
            let total: u64 = b.iter().map(|&(_, n)| u64::from(n)).sum::<u64>().max(1);
            let mut sum = [0u64; 3];
            for &(c, n) in b {
                for (s, v) in sum.iter_mut().zip(channels(c)) {
                    *s += v as u64 * u64::from(n);
                }
            }
            let [r, g, b] = sum.map(|s| (s / total) as u32);
            r << 16 | g << 8 | b
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_input::{Action, Actions};
    use clamber_level::Level;
    use clamber_sim::{World, generic_feel};

    use super::*;
    use crate::canvas::rgb;
    use crate::game::Game;
    use crate::surface::Surface;

    /// A session at the title screen of a flat level with a goal, and a
    /// script that starts and walks right into it.
    fn walk_to_goal() -> (Session, Script) {
        let level = Level::from_toml_str(
            r#"
            format = 1
            size = [1000, 400]
            start = [500, 300]

            [[collision]]
            shape = "rect"
            rect = [0, 300, 1000, 100]

            [[entities]]
            type = "goal"
            at = [600, 300]
            "#,
            "test",
        )
        .unwrap();
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let mut script = Script::idle(240);
        script.held[10] = Action::Jump.into();
        for held in &mut script.held[11..] {
            *held = Actions::from(Action::Right);
        }
        (Session::new(Game::new(world)), script)
    }

    #[test]
    fn captures_a_run_to_the_goal() {
        let (mut session, script) = walk_to_goal();
        let run = capture(&mut session, &script, 1);
        assert_eq!(run.frames.len(), 100);
        assert_eq!(run.shots.len(), SHOTS);
        assert_eq!(run.shots[0], 0, "the title screen");
        assert!(matches!(session.screen, crate::Screen::Complete(_)));
        assert!(run.shots.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn redraws_the_same_frames_at_another_scale() {
        let (mut session, script) = walk_to_goal();
        let run = capture(&mut session, &script, 1);
        let wanted = [0, 37, 99];
        let (mut session, _) = walk_to_goal();
        let same = redraw(&mut session, &script, &wanted, 1);
        let want: Vec<_> = wanted.iter().map(|&i| run.frames[i].clone()).collect();
        assert_eq!(same, want);

        let (mut session, _) = walk_to_goal();
        let larger = redraw(&mut session, &script, &wanted[1..], 2);
        assert_eq!(larger.len(), 2);
        assert_eq!(
            (larger[0].width, larger[0].height),
            (2 * VIEW.0, 2 * VIEW.1)
        );
    }

    #[test]
    fn keeps_25_frames_a_second() {
        let kept = (0u32..60).filter(|&s| is_kept(s)).count();
        assert_eq!(kept, 25);
    }

    #[test]
    fn shots_prefer_lively_moments_kept_apart() {
        // Two jumps close together and a pickup: the pickup, one jump,
        // then the evenly spaced frames (12, 37, 62, 87) that aren't
        // too close to those.
        let shots = pick_shots(&[(1, 30), (1, 34), (2, 70)], 100, 4);
        assert_eq!(shots, [12, 30, 70, 87]);
    }

    #[test]
    fn fills_in_with_even_frames() {
        assert_eq!(pick_shots(&[], 80, 4), [10, 30, 50, 70]);
        assert_eq!(pick_shots(&[], 0, 4), Vec::<usize>::new());
    }

    #[test]
    fn small_palettes_are_exact() {
        let mut canvas = Canvas::new(2, 1);
        canvas.pixels = vec![rgb(1, 2, 3), rgb(200, 100, 0)];
        let palette = Palette::new(&[canvas]);
        assert_eq!(palette.colors.len(), 2);
        assert_eq!(
            palette.colors[palette.index(rgb(200, 100, 0)) as usize],
            rgb(200, 100, 0)
        );
    }

    #[test]
    fn large_palettes_are_reduced() {
        let mut canvas = Canvas::new(1000, 1);
        canvas.pixels = (0..1000).map(|i| i * 16_000).collect();
        let palette = Palette::new(&[canvas.clone()]);
        assert_eq!(palette.colors.len(), COLORS);
        for &c in &canvas.pixels {
            let [r, g, b] = channels(c);
            let [r2, g2, b2] = channels(palette.colors[palette.index(c) as usize]);
            assert!((r - r2).abs() + (g - g2).abs() + (b - b2).abs() < 120);
        }
    }

    #[test]
    fn gif_round_trips_through_a_decoder() {
        let mut a = Canvas::new(8, 4);
        a.clear(rgb(10, 20, 30));
        let mut b = a.clone();
        b.plot(5, 2, rgb(250, 0, 0));
        let bytes = encode_gif(&[a, b], 2, GIF_DELAY).unwrap();

        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(bytes.as_slice()).unwrap();
        assert_eq!((decoder.width(), decoder.height()), (16, 8));
        let first = decoder.read_next_frame().unwrap().unwrap();
        assert_eq!((first.width, first.height, first.delay), (16, 8, GIF_DELAY));
        let second = decoder.read_next_frame().unwrap().unwrap();
        assert_eq!(
            (second.left, second.top, second.width, second.height),
            (10, 4, 2, 2)
        );
        assert_eq!(&second.buffer[..4], &[250, 0, 0, 255]);
        assert!(decoder.read_next_frame().unwrap().is_none());
    }

    /// Every frame of a GIF, composited, as `0x00RRGGBB` pixels.
    fn decode_gif(bytes: &[u8]) -> (u32, Vec<Vec<Color>>) {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(bytes).unwrap();
        let width = u32::from(decoder.width());
        let mut screen = vec![0; width as usize * usize::from(decoder.height())];
        let mut frames = Vec::new();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            for (i, px) in frame.buffer.chunks(4).enumerate() {
                if px[3] == 0 {
                    continue;
                }
                let x = u32::from(frame.left) + i as u32 % u32::from(frame.width);
                let y = u32::from(frame.top) + i as u32 / u32::from(frame.width);
                screen[(y * width + x) as usize] = rgb(px[0], px[1], px[2]);
            }
            frames.push(screen.clone());
        }
        (width, frames)
    }

    #[test]
    fn gif_frames_composite_to_the_originals() {
        let mut frames = Vec::new();
        let mut canvas = Canvas::new(20, 10);
        for i in 0..6 {
            canvas.fill_rect(i * 3, i, 4, 3, rgb(40 * i as u8, 255 - 30 * i as u8, 7));
            frames.push(canvas.clone());
            // A repeat: nothing changes.
            frames.push(canvas.clone());
        }
        let (width, decoded) = decode_gif(&encode_gif(&frames, 3, GIF_DELAY).unwrap());
        assert_eq!(width, 60);
        assert_eq!(decoded.len(), frames.len());
        for (canvas, got) in frames.iter().zip(&decoded) {
            for y in 0..30 {
                for x in 0..60 {
                    let want = canvas.get(x / 3, y / 3).unwrap();
                    assert_eq!(got[(y * 60 + x) as usize], want, "at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn png_is_scaled() {
        let mut canvas = Canvas::new(3, 2);
        canvas.plot(2, 1, rgb(1, 2, 3));
        let bytes = encode_png(&canvas, 2).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (6, 4));
    }
}
