//! A list of choices navigated with neutral actions, so the same menu
//! works from a keyboard or a gamepad alone: Up and Down move the
//! selection (wrapping around), Jump confirms and Pause goes back. The
//! session gives menus the menu controls (`Input::menu`): Enter confirms
//! and Escape goes back, not the game's jump and pause.
//! Unavailable items can be selected, so they can be read, but not
//! picked.
//!
//! Menus are drawn on a rendered panel that fades in, with a wooden
//! highlight behind the selected item that slides to the next one, and a
//! turning gem on it (see `panel`).

use clamber_input::{Action, Frame};

use crate::canvas::{Color, rgb};
use crate::panel::{INSET, draw_cursor, draw_highlight, draw_panel};
use crate::surface::Surface;
use crate::text::{GLYPH, LINE, draw_text_centred, text_size};

const ITEM: Color = rgb(0xa8, 0xb0, 0xbc);
const SELECTED: Color = rgb(0xf4, 0xf0, 0xe8);
const UNAVAILABLE: Color = rgb(0x6c, 0x72, 0x7c);

/// The narrowest the items' column is, in view units, so short menus
/// still look like menus.
const MIN_WIDTH: f32 = 72.0;
/// How far the highlight reaches past the widest item on each side; the
/// cursor sits on its left end.
pub const REACH: f32 = 14.0;
/// From the highlight's left end to the cursor's middle.
pub const CURSOR_IN: f32 = 7.0;
/// Frames the panel takes to fade in, and the highlight to slide.
const FADE: u32 = 8;
const SLIDE: u32 = 6;

/// Extra space between items, beyond a line of text.
const SPACING: i32 = 3;

/// What a frame of input did to a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice<T> {
    /// The selected item was confirmed.
    Picked(T),
    /// Pause was pressed: leave the menu without picking.
    Back,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu<T> {
    pub items: Vec<(String, T)>,
    pub selected: usize,
    /// Indices of the items that can't be picked.
    pub unavailable: Vec<usize>,
    /// Frames it's been updated for, which time its animations.
    pub age: u32,
    /// The item selected before the last move and the age it moved at,
    /// for the highlight to slide from.
    moved: Option<(usize, u32)>,
}

impl<T: Copy> Menu<T> {
    /// A menu with the first item selected.
    pub fn new(items: impl IntoIterator<Item = (impl Into<String>, T)>) -> Menu<T> {
        Menu {
            items: items.into_iter().map(|(l, t)| (l.into(), t)).collect(),
            selected: 0,
            unavailable: Vec::new(),
            age: 0,
            moved: None,
        }
    }

    /// The menu with the items at `indices` shown, but not pickable.
    pub fn with_unavailable(self, indices: impl IntoIterator<Item = usize>) -> Menu<T> {
        Menu {
            unavailable: indices.into_iter().collect(),
            ..self
        }
    }

    /// The menu with the item at `index` selected.
    pub fn with_selected(self, index: usize) -> Menu<T> {
        assert!(index < self.items.len());
        Menu {
            selected: index,
            ..self
        }
    }

    pub fn available(&self, index: usize) -> bool {
        !self.unavailable.contains(&index)
    }

    pub fn current(&self) -> T {
        self.items[self.selected].1
    }

    /// Applies one frame of input: moves on Up or Down, and reports a
    /// pick on Jump or leaving on Pause. Only presses count, so holding
    /// a button does nothing more. Jump on an unavailable item does
    /// nothing.
    pub fn update(&mut self, frame: &Frame) -> Option<Choice<T>> {
        self.age = self.age.saturating_add(1);
        let pressed = frame.pressed;
        if pressed.contains(Action::Pause) {
            return Some(Choice::Back);
        }
        if pressed.contains(Action::Jump) {
            return self
                .available(self.selected)
                .then(|| Choice::Picked(self.current()));
        }
        let n = self.items.len();
        let was = self.selected;
        if pressed.contains(Action::Up) && !pressed.contains(Action::Down) {
            self.selected = (self.selected + n - 1) % n;
        } else if pressed.contains(Action::Down) && !pressed.contains(Action::Up) {
            self.selected = (self.selected + 1) % n;
        }
        if self.selected != was {
            self.moved = Some((was, self.age));
        }
        None
    }

    /// Height of the drawn items, px.
    pub fn height(&self) -> i32 {
        Menu::<T>::rows_height(self.items.len())
    }

    /// Draws the items centred on `centre_x` from `top` down, on a panel,
    /// with the selected one highlighted.
    pub fn draw(&self, canvas: &mut dyn Surface, centre_x: i32, top: i32) {
        self.draw_rows(canvas, centre_x, top, self.items.len());
    }

    /// Height of `rows` drawn items, px.
    pub fn rows_height(rows: usize) -> i32 {
        rows as i32 * (LINE + SPACING) - SPACING
    }

    /// The first of `rows` items shown at once, so that the selected one
    /// is among them, as near the middle as the ends allow.
    pub fn first_shown(&self, rows: usize) -> usize {
        let rows = rows.max(1);
        self.selected
            .saturating_sub(rows / 2)
            .min(self.items.len().saturating_sub(rows))
    }

    /// Like [`draw`](Menu::draw), but only `rows` items at most, scrolled
    /// to show the selected one.
    pub fn draw_rows(&self, canvas: &mut dyn Surface, centre_x: i32, top: i32, rows: usize) {
        let first = self.first_shown(rows);
        let shown: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .skip(first)
            .take(rows)
            .collect();
        let widest = shown
            .iter()
            .map(|(_, (label, _))| text_size(label).0 as f32)
            .fold(MIN_WIDTH, f32::max);
        let (centre, top_f) = (centre_x as f32, top as f32);
        let half = widest / 2.0 + REACH;
        let height = Menu::<T>::rows_height(shown.len()) as f32;
        let fade = self.age.min(FADE) as f32 / FADE as f32;
        draw_panel(
            canvas,
            [
                centre - half - INSET,
                top_f - INSET,
                centre + half + INSET,
                top_f + height + INSET,
            ],
            (255.0 * fade) as u8,
        );

        let row_top = |i: usize| top_f + (i as f32 - first as f32) * (LINE + SPACING) as f32;
        let mut y = row_top(self.selected);
        if let Some((from, at)) = self.moved {
            let t = (self.age - at) as f32 / SLIDE as f32;
            if t < 1.0 && (first..first + shown.len()).contains(&from) {
                let eased = 1.0 - (1.0 - t) * (1.0 - t);
                y = row_top(from) + (y - row_top(from)) * eased;
            }
        }
        // A slow, slight pulse.
        let pulse = (self.age as f32 * std::f32::consts::TAU / 90.0).sin();
        let opacity = match self.available(self.selected) {
            true => 200.0 + 40.0 * pulse,
            false => 110.0,
        };
        let bar = [
            centre - half,
            y - 2.0,
            centre + half,
            y + GLYPH.1 as f32 + 2.0,
        ];
        draw_highlight(canvas, bar, (opacity * fade) as u8);
        let middle = (bar[1] + bar[3]) / 2.0;
        draw_cursor(canvas, (bar[0] + CURSOR_IN, middle), self.age);

        for (row, &(i, (label, _))) in shown.iter().enumerate() {
            let y = top + row as i32 * (LINE + SPACING);
            let color = match (self.available(i), i == self.selected) {
                (false, _) => UNAVAILABLE,
                (true, true) => SELECTED,
                (true, false) => ITEM,
            };
            draw_text_centred(canvas, centre_x, y, label, color);
        }
    }
}

/// What the mouse needs of a menu, whatever its items are.
pub trait Pointable {
    /// The item under `pos` (view units) in the menu drawn with
    /// [`Menu::draw_rows`] at `centre_x`, `top` and `rows`, if any.
    fn at(&self, centre_x: i32, top: i32, rows: usize, pos: (f32, f32)) -> Option<usize>;
    /// The left and right ends of the highlight when drawn like that.
    fn span(&self, centre_x: i32, rows: usize) -> (f32, f32);
    /// Moves the selection to `index`, sliding the highlight there.
    fn select(&mut self, index: usize);
}

impl<T: Copy> Pointable for Menu<T> {
    fn at(&self, centre_x: i32, top: i32, rows: usize, pos: (f32, f32)) -> Option<usize> {
        let (left, right) = self.span(centre_x, rows);
        let shown = self.items.len().min(rows.max(1));
        let pitch = (LINE + SPACING) as f32;
        let row = ((pos.1 - top as f32 + SPACING as f32 / 2.0) / pitch).floor();
        let inside = (left..=right).contains(&pos.0) && row >= 0.0 && (row as usize) < shown;
        inside.then(|| self.first_shown(rows) + row as usize)
    }

    fn span(&self, centre_x: i32, rows: usize) -> (f32, f32) {
        let first = self.first_shown(rows);
        let widest = self
            .items
            .iter()
            .skip(first)
            .take(rows.max(1))
            .map(|(label, _)| text_size(label).0 as f32)
            .fold(MIN_WIDTH, f32::max);
        let half = widest / 2.0 + REACH;
        (centre_x as f32 - half, centre_x as f32 + half)
    }

    fn select(&mut self, index: usize) {
        if index < self.items.len() && index != self.selected {
            self.moved = Some((self.selected, self.age));
            self.selected = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use clamber_input::Actions;

    use super::*;
    use crate::canvas::Canvas;

    fn press(action: Action) -> Frame {
        Frame::after(Actions::NONE, action.into())
    }

    fn menu() -> Menu<u8> {
        Menu::new([("One", 1), ("Two", 2), ("Three", 3)])
    }

    #[test]
    fn the_pointer_finds_the_item_under_it() {
        let mut m = menu();
        let pitch = (LINE + SPACING) as f32;
        let (left, right) = m.span(192, 3);
        assert_eq!(m.at(192, 50, 3, (192.0, 51.0)), Some(0));
        assert_eq!(m.at(192, 50, 3, (192.0, 50.0 + pitch + 2.0)), Some(1));
        assert_eq!(
            m.at(192, 50, 3, (right - 1.0, 50.0 + 2.0 * pitch + 3.0)),
            Some(2)
        );
        assert_eq!(m.at(192, 50, 3, (left - 1.0, 51.0)), None);
        assert_eq!(m.at(192, 50, 3, (192.0, 49.0 - SPACING as f32)), None);
        assert_eq!(m.at(192, 50, 3, (192.0, 50.0 + 3.0 * pitch)), None);
        m.select(2);
        assert_eq!(m.current(), 3);
    }

    #[test]
    fn moves_and_wraps() {
        let mut menu = menu();
        assert_eq!(menu.update(&press(Action::Down)), None);
        assert_eq!(menu.current(), 2);
        menu.update(&press(Action::Down));
        menu.update(&press(Action::Down));
        assert_eq!(menu.current(), 1, "wraps past the bottom");
        menu.update(&press(Action::Up));
        assert_eq!(menu.current(), 3, "wraps past the top");
    }

    #[test]
    fn picks_and_goes_back() {
        let mut menu = menu();
        menu.update(&press(Action::Down));
        assert_eq!(menu.update(&press(Action::Jump)), Some(Choice::Picked(2)));
        assert_eq!(menu.update(&press(Action::Pause)), Some(Choice::Back));
    }

    #[test]
    fn holding_does_nothing_more() {
        let mut menu = menu();
        let down = Actions::from(Action::Down);
        menu.update(&Frame::after(Actions::NONE, down));
        for _ in 0..30 {
            assert_eq!(menu.update(&Frame::after(down, down)), None);
        }
        assert_eq!(menu.current(), 2);
        let jump = Actions::from(Action::Jump);
        assert_eq!(menu.update(&Frame::after(jump, jump)), None);
    }

    #[test]
    fn unavailable_items_are_selected_but_not_picked() {
        let mut menu = menu().with_unavailable([1]).with_selected(2);
        assert_eq!(menu.current(), 3);
        menu.update(&press(Action::Up));
        assert_eq!(menu.current(), 2);
        assert_eq!(menu.update(&press(Action::Jump)), None);
        menu.update(&press(Action::Up));
        assert_eq!(menu.update(&press(Action::Jump)), Some(Choice::Picked(1)));

        let mut canvas = Canvas::new(100, 60);
        menu.draw(&mut canvas, 50, 10);
        assert!(canvas.pixels.contains(&UNAVAILABLE));
    }

    #[test]
    fn marks_the_selected_item() {
        let mut menu = menu();
        let mut canvas = Canvas::scaled((100, 60), 4);
        menu.draw(&mut canvas, 50, 10);
        assert!(canvas.pixels.contains(&SELECTED));
        let first = canvas.clone();
        menu.update(&press(Action::Down));
        menu.draw(&mut canvas, 50, 10);
        assert_ne!(canvas, first);
        assert_eq!(menu.height(), 3 * LINE + 2 * SPACING);
    }

    #[test]
    fn the_panel_fades_in_and_the_highlight_slides() {
        let draw = |menu: &Menu<u8>| {
            let mut canvas = Canvas::scaled((120, 70), 2);
            menu.draw(&mut canvas, 60, 15);
            canvas
        };
        let mut menu = menu();
        let none = Actions::NONE;
        let fresh = draw(&menu);
        for _ in 0..FADE {
            menu.update(&Frame::after(none, none));
        }
        let shown = draw(&menu);
        assert_ne!(fresh, shown, "faded in");
        // The left of the panel, where neither cursor nor text is.
        let at = |c: &Canvas| c.pixels[30 * c.width as usize + 12];
        assert_eq!(at(&fresh), rgb(0, 0, 0));
        assert_ne!(at(&shown), rgb(0, 0, 0));

        menu.update(&press(Action::Down));
        let sliding = draw(&menu);
        for _ in 0..SLIDE {
            menu.update(&Frame::after(none, none));
        }
        let still = draw(&menu);
        menu.update(&Frame::after(none, none));
        assert_ne!(sliding, still);
        let mut moved = menu.clone();
        moved.moved = None;
        // The highlight has come to rest, pulsing alike either way.
        assert_eq!(draw(&menu), draw(&moved));
    }

    #[test]
    fn scrolls_to_show_the_selected_item() {
        let mut menu = Menu::new((0..10).map(|i| (i.to_string(), i)));
        assert_eq!(menu.first_shown(4), 0);
        menu.selected = 5;
        assert_eq!(menu.first_shown(4), 3);
        menu.selected = 9;
        assert_eq!(menu.first_shown(4), 6);
        assert_eq!(menu.first_shown(20), 0);

        let mut canvas = Canvas::new(100, 60);
        menu.draw_rows(&mut canvas, 50, 0, 4);
        let mut all = Canvas::new(100, 60);
        menu.draw_rows(&mut all, 50, 0, 10);
        assert_ne!(canvas, all);
    }
}
