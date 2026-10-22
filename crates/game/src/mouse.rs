//! The mouse in the menus: hovering selects, clicking picks, a volume
//! can be dragged, the wheel moves the selection and a right click goes
//! back. The mouse only adds to what the neutral actions do (see
//! `Session::tick`), so the menus work the same without one.
//!
//! The selection follows the pointer when it moves, and only then: a
//! key or a pad button moves the selection from where it is, and the
//! pointer's place is ignored until it moves again.

use clamber_input::Action;

use crate::camera::VIEW;
use crate::controls::Row;
use crate::import::{GAMES_ROWS, GAMES_TOP, PICKER_SHOWN, PICKER_TOP};
use crate::menu::{Menu, Pointable};
use crate::prefs::Volume;
use crate::session::{
    COMPLETE_MENU, GAME_OVER_MENU, MOVEMENT_MENU, OPTIONS_MENU, PAUSED_MENU, Screen, Session,
    Setting, TITLE_MENU, options_items,
};

/// Something the mouse did, in view units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mouse {
    /// The pointer moved to this place.
    Moved(f32, f32),
    /// The main button went down or up.
    Left { down: bool },
    /// The secondary button went down or up.
    Right { down: bool },
    /// The wheel turned: positive away from the player.
    Wheel(f32),
}

impl Session {
    /// The menu showing that the mouse can point at, where its items
    /// start and how many show at once.
    fn pointed(&mut self) -> Option<(&mut dyn Pointable, i32, usize)> {
        fn all<T: Copy>(menu: &mut Menu<T>, top: i32) -> Option<(&mut dyn Pointable, i32, usize)> {
            let rows = menu.items.len();
            Some((menu, top, rows))
        }
        match &mut self.screen {
            Screen::Title(m) => all(m, TITLE_MENU),
            Screen::Paused(m) => all(m, PAUSED_MENU),
            Screen::Complete(m) => all(m, COMPLETE_MENU),
            Screen::GameOver(m) => all(m, GAME_OVER_MENU),
            Screen::Movement(m, _) => all(m, MOVEMENT_MENU),
            Screen::Options(m, _) => all(m, OPTIONS_MENU),
            Screen::Picker(p, _) if p.typed.is_none() => {
                Some((&mut p.menu, PICKER_TOP, PICKER_SHOWN))
            }
            Screen::Games(g, _) => Some((&mut g.menu, GAMES_TOP, GAMES_ROWS)),
            Screen::Importing(i, _) => {
                let menu = i.menu.as_mut()?;
                let top = VIEW.1 as i32 - 20 - menu.height();
                all(menu, top)
            }
            _ => None,
        }
    }

    fn centre(&self) -> i32 {
        self.look().view.0 as i32 / 2
    }

    /// Takes a mouse event. Nothing happens in play.
    pub fn mouse(&mut self, mouse: Mouse) {
        if matches!(self.screen, Screen::Playing | Screen::Map) {
            self.dragging = None;
            if let Mouse::Moved(x, y) = mouse {
                self.pointer = Some((x, y));
            }
            return;
        }
        let centre = self.centre();
        match mouse {
            Mouse::Moved(x, y) => {
                self.pointer = Some((x, y));
                if let Some(setting) = self.dragging {
                    self.drag_volume(setting, x);
                } else if let Screen::Controls(controls, _) = &mut self.screen {
                    if self.input.remapping().is_none() {
                        controls.point((x, y));
                    }
                } else if let Some((menu, top, rows)) = self.pointed()
                    && let Some(i) = menu.at(centre, top, rows, (x, y))
                {
                    menu.select(i);
                }
            }
            Mouse::Left { down: false } => {
                if self.dragging.take().is_some() {
                    self.save_prefs("the volume");
                }
            }
            Mouse::Left { down: true } => self.click(centre),
            Mouse::Right { down: true } => self.clicks.insert(Action::Pause),
            Mouse::Right { down: false } => {}
            Mouse::Wheel(dy) if dy > 0.0 => self.clicks.insert(Action::Up),
            Mouse::Wheel(dy) if dy < 0.0 => self.clicks.insert(Action::Down),
            Mouse::Wheel(_) => {}
        }
    }

    /// Picks what's under the pointer.
    fn click(&mut self, centre: i32) {
        let Some(pos) = self.pointer else {
            return;
        };
        if let Screen::Credits(..) = self.screen {
            self.clicks.insert(Action::Jump);
            return;
        }
        if let Screen::Controls(controls, _) = &mut self.screen {
            if self.input.remapping().is_none() && controls.point(pos) {
                let pick = match controls.row() {
                    Row::Player if pos.0 < centre as f32 => Action::Left,
                    Row::Player => Action::Right,
                    _ => Action::Jump,
                };
                self.clicks.insert(pick);
            }
            return;
        }
        let Some((menu, top, rows)) = self.pointed() else {
            return;
        };
        let Some(i) = menu.at(centre, top, rows, pos) else {
            return;
        };
        menu.select(i);
        let slider = match &self.screen {
            Screen::Options(menu, _) => match menu.items[i].1 {
                s @ (Setting::MusicVolume | Setting::EffectsVolume) => Some(s),
                _ => None,
            },
            _ => None,
        };
        match slider {
            Some(setting) => {
                self.dragging = Some(setting);
                self.drag_volume(setting, pos.0);
            }
            None => self.clicks.insert(Action::Jump),
        }
    }

    /// Sets a volume from how far along the highlight `x` is, in steps.
    fn drag_volume(&mut self, setting: Setting, x: f32) {
        let centre = self.centre();
        let Screen::Options(menu, _) = &self.screen else {
            self.dragging = None;
            return;
        };
        let (left, right) = menu.span(centre, menu.items.len());
        let step = f32::from(Volume::STEP);
        let along = ((x - left) / (right - left)).clamp(0.0, 1.0);
        let percent = ((along * 100.0 / step).round() * step) as u8;
        let volume = &mut self.prefs.volume;
        let level = match setting {
            Setting::MusicVolume => &mut volume.music,
            Setting::EffectsVolume => &mut volume.effects,
            _ => return,
        };
        if *level == percent {
            return;
        }
        *level = percent;
        let items = options_items(&self.prefs, self.widescreen(), self.rendering());
        if let Screen::Options(menu, _) = &mut self.screen {
            menu.items = items;
        }
    }
}
