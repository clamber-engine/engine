//! A list of choices navigated with neutral actions, so the same menu
//! works from a keyboard or a gamepad alone: Up and Down move the
//! selection (wrapping around), Jump confirms and Pause goes back.
//! Unavailable items can be selected, so they can be read, but not
//! picked.

use clamber_input::{Action, Frame};

use crate::canvas::{Color, rgb};
use crate::surface::Surface;
use crate::text::{ADVANCE, LINE, draw_text, draw_text_centred, text_size};

const ITEM: Color = rgb(0xa8, 0xb0, 0xbc);
const SELECTED: Color = rgb(0xf4, 0xf0, 0xe8);
const MARKER: Color = rgb(0xf0, 0xc8, 0x40);
const UNAVAILABLE: Color = rgb(0x58, 0x5e, 0x68);

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
}

impl<T: Copy> Menu<T> {
    /// A menu with the first item selected.
    pub fn new(items: impl IntoIterator<Item = (impl Into<String>, T)>) -> Menu<T> {
        Menu {
            items: items.into_iter().map(|(l, t)| (l.into(), t)).collect(),
            selected: 0,
            unavailable: Vec::new(),
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
        if pressed.contains(Action::Up) && !pressed.contains(Action::Down) {
            self.selected = (self.selected + n - 1) % n;
        } else if pressed.contains(Action::Down) && !pressed.contains(Action::Up) {
            self.selected = (self.selected + 1) % n;
        }
        None
    }

    /// Height of the drawn items, px.
    pub fn height(&self) -> i32 {
        Menu::<T>::rows_height(self.items.len())
    }

    /// Draws the items centred on `centre_x` from `top` down, with a
    /// marker beside the selected one.
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
        let shown = self.items.iter().enumerate().skip(first).take(rows);
        for (row, (i, (label, _))) in shown.enumerate() {
            let y = top + row as i32 * (LINE + SPACING);
            let color = match (self.available(i), i == self.selected) {
                (false, _) => UNAVAILABLE,
                (true, true) => SELECTED,
                (true, false) => ITEM,
            };
            draw_text_centred(canvas, centre_x, y, label, color);
            if i == self.selected {
                let left = centre_x - text_size(label).0 / 2;
                draw_text(canvas, left - 2 * ADVANCE, y, ">", MARKER);
            }
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
        let mut canvas = Canvas::new(100, 60);
        menu.draw(&mut canvas, 50, 10);
        assert!(canvas.pixels.contains(&SELECTED));
        assert!(canvas.pixels.contains(&MARKER));
        let first = canvas.clone();
        menu.update(&press(Action::Down));
        menu.draw(&mut canvas, 50, 10);
        assert_ne!(canvas, first);
        assert_eq!(menu.height(), 3 * LINE + 2 * SPACING);
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
