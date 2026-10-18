//! Game packs: a roster of characters, the presets the pack adds, and the
//! preset the pack was designed for.

use std::collections::BTreeMap;
use std::fmt;

use clamber_fixed::Fixed;
use serde::Deserialize;
use toml::Table;

use crate::load::{LoadError, Reader};
use crate::physics::{Physics, PhysicsPatch};
use crate::preset::{DEFAULT, Presets, read_presets};

/// Ability parameters, by name.
pub type AbilityParams = BTreeMap<String, Fixed>;

/// A playable character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Character {
    pub id: String,
    pub name: String,
    /// How this character's movement differs from the world's.
    pub profile: PhysicsPatch,
    /// What the character can do, by ability name, with each ability's
    /// parameters. The engine implements abilities; packs choose which
    /// characters get them and tune them.
    pub abilities: BTreeMap<String, AbilityParams>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub id: String,
    pub name: String,
    /// The preset the pack's levels were designed for.
    pub preset: String,
    /// Playable characters by slot: `roster[0]`, `roster[1]`, …
    pub roster: Vec<Character>,
    /// The built-in presets followed by the pack's own.
    presets: Presets,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPack {
    id: String,
    name: String,
    preset: Option<String>,
    roster: Vec<RawCharacter>,
    #[serde(default)]
    presets: Table,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCharacter {
    id: String,
    name: String,
    #[serde(default)]
    profile: Table,
    #[serde(default)]
    abilities: Table,
}

impl Pack {
    /// Reads a `pack.toml`.
    pub fn from_toml_str(src: &str, origin: &str) -> Result<Pack, LoadError> {
        let reader = Reader { origin };
        let raw: RawPack = reader.parse_document(src)?;
        reader.id("id", &raw.id)?;

        if raw.roster.is_empty() {
            return Err(reader.err("roster", "a pack needs at least one character"));
        }
        let mut roster: Vec<Character> = Vec::new();
        for (slot, raw) in raw.roster.into_iter().enumerate() {
            let path = format!("roster[{slot}]");
            reader.id(&format!("{path}.id"), &raw.id)?;
            if roster.iter().any(|c| c.id == raw.id) {
                return Err(reader.err(
                    &format!("{path}.id"),
                    format!("`{}` is already in the roster", raw.id),
                ));
            }
            let mut abilities = BTreeMap::new();
            for (name, params) in &raw.abilities {
                let at = format!("{path}.abilities.{name}");
                reader.name(&at, name)?;
                abilities.insert(
                    name.clone(),
                    reader.params(&at, reader.table(&at, params)?)?,
                );
            }
            roster.push(Character {
                id: raw.id,
                name: raw.name,
                profile: reader.physics_patch(&format!("{path}.profile"), &raw.profile)?,
                abilities,
            });
        }

        let presets = Presets::builtin().extended(&reader, read_presets(&reader, &raw.presets)?)?;
        let preset = raw.preset.unwrap_or_else(|| DEFAULT.to_string());
        if presets.get(&preset).is_none() {
            return Err(reader.err("preset", format!("no preset named `{preset}`")));
        }

        Ok(Pack {
            id: raw.id,
            name: raw.name,
            preset,
            roster,
            presets,
        })
    }

    /// Every preset a player can choose for this pack.
    pub fn presets(&self) -> &Presets {
        &self.presets
    }

    /// Resolves the physics for every character under a preset: the one
    /// the pack was designed for, or `choice`. Choosing another preset is
    /// allowed but comes with a warning.
    pub fn select_feel(&self, choice: Option<&str>) -> Result<Selection, UnknownPreset> {
        let id = choice.unwrap_or(&self.preset);
        let chain = self
            .presets
            .chain(id)
            .ok_or_else(|| UnknownPreset(id.to_string()))?;
        let preset = chain.last().expect("a chain includes the preset itself");

        let mut warnings = Vec::new();
        if id != self.preset {
            warnings.push(FeelWarning::NotDesignedFor {
                designed_for: self.preset.clone(),
                chosen: id.to_string(),
            });
        }
        if preset.placeholder {
            warnings.push(FeelWarning::Placeholder {
                preset: id.to_string(),
            });
        }

        let mut world = Physics::default();
        for p in &chain {
            world.apply(&p.world);
        }

        // world, then the character's own profile, then each preset's
        // adjustments for the slot, root first.
        let roster = self
            .roster
            .iter()
            .enumerate()
            .map(|(slot, character)| {
                let mut physics = world.patched(&character.profile);
                let mut abilities = character.abilities.clone();
                for slot_patch in chain.iter().filter_map(|p| p.slots.get(&slot)) {
                    physics.apply(&slot_patch.physics);
                    for (name, patch) in &slot_patch.abilities {
                        if let Some(params) = abilities.get_mut(name) {
                            patch.apply_to(params);
                        }
                    }
                }
                CharacterFeel { physics, abilities }
            })
            .collect();

        Ok(Selection {
            feel: Feel {
                preset: id.to_string(),
                title: preset.title.clone(),
                world,
                roster,
                window: None,
            },
            warnings,
        })
    }
}

/// Resolved physics for a pack under one preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feel {
    pub preset: String,
    /// The preset's [`Preset::title`](crate::Preset::title): the title
    /// a trace recorded on this feel represents.
    pub title: Option<String>,
    pub world: Physics,
    /// By roster slot.
    pub roster: Vec<CharacterFeel>,
    /// How wide, in whole px, a window wider than the movement's view
    /// (`view_width`) is that things appear, act and are removed by in
    /// place of the view, when the game is played in wide activation.
    /// The engine's own, set by the game for how it's shown: no preset
    /// sets it, and with none things go by the view.
    pub window: Option<u32>,
}

/// One character's resolved movement and abilities. Level regions adjust
/// this further with [`Physics::patched`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterFeel {
    pub physics: Physics,
    pub abilities: BTreeMap<String, AbilityParams>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub feel: Feel,
    /// To show the player before play starts.
    pub warnings: Vec<FeelWarning>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeelWarning {
    /// The player chose a preset the pack wasn't designed for.
    NotDesignedFor {
        designed_for: String,
        chosen: String,
    },
    /// The preset's values aren't final.
    Placeholder { preset: String },
}

impl fmt::Display for FeelWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeelWarning::NotDesignedFor {
                designed_for,
                chosen,
            } => write!(
                f,
                "this pack was designed for the `{designed_for}` preset; with `{chosen}`, \
                 some jumps and level layouts may not work as intended"
            ),
            FeelWarning::Placeholder { preset } => {
                write!(
                    f,
                    "the `{preset}` preset is a placeholder and its values may change"
                )
            }
        }
    }
}

/// [`Pack::select_feel`] was given a preset that doesn't exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownPreset(pub String);

impl fmt::Display for UnknownPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no preset named `{}`", self.0)
    }
}

impl std::error::Error for UnknownPreset {}
