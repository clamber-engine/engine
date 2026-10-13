//! The movement presets a player can choose from, and the feel each
//! gives: with one generic character, or a pack's roster.

use std::sync::Arc;

use clamber_pack::{Feel, Pack, Presets};
use clamber_sim::generic_feel;

/// The preset the bundled game starts on, with its generic character.
pub const STARTING_PRESET: &str = "classic-1";

#[derive(Clone, Debug, Default)]
pub struct Feels {
    /// `None` for the built-in presets and a generic character.
    pack: Option<Arc<Pack>>,
}

impl Feels {
    /// The built-in presets, with one character that can roll.
    pub fn generic() -> Feels {
        Feels::default()
    }

    /// `pack`'s roster under its presets.
    pub fn pack(pack: Pack) -> Feels {
        Feels {
            pack: Some(Arc::new(pack)),
        }
    }

    /// Every preset there is to choose.
    pub fn presets(&self) -> &Presets {
        match &self.pack {
            Some(pack) => pack.presets(),
            None => Presets::builtin(),
        }
    }

    /// The preset played when none is chosen: the pack's own, or
    /// [`STARTING_PRESET`].
    pub fn designed_for(&self) -> &str {
        self.pack
            .as_ref()
            .map_or(STARTING_PRESET, |pack| &pack.preset)
    }

    /// The feel under preset `id`, and warnings to show about it.
    pub fn feel(&self, id: &str) -> Result<(Feel, Vec<String>), String> {
        match &self.pack {
            Some(pack) => {
                let selection = pack.select_feel(Some(id)).map_err(|e| e.to_string())?;
                let warnings = selection.warnings.iter().map(|w| w.to_string()).collect();
                Ok((selection.feel, warnings))
            }
            None => {
                let feel = generic_feel(id).ok_or_else(|| format!("no preset named `{id}`"))?;
                let placeholder = self.presets().get(id).is_some_and(|p| p.placeholder);
                let warnings = placeholder
                    .then(|| {
                        format!("the `{id}` preset is a placeholder and its values may change")
                    })
                    .into_iter()
                    .collect();
                Ok((feel, warnings))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_feels_come_from_the_builtin_presets() {
        let feels = Feels::generic();
        assert_eq!(feels.designed_for(), "classic-1");
        let (feel, warnings) = feels.feel(STARTING_PRESET).unwrap();
        assert_eq!(feel, generic_feel(STARTING_PRESET).unwrap());
        assert!(warnings.is_empty(), "{warnings:?}");
        let (_, warnings) = feels.feel("classic-2").unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(
            feels.feel("nope").unwrap_err(),
            "no preset named `nope`".to_string()
        );
    }

    #[test]
    fn pack_feels_use_its_roster_and_preset() {
        let src = r#"
            id = "test"
            name = "Test"
            preset = "snappy"

            [[roster]]
            id = "one"
            name = "One"

            [[roster]]
            id = "two"
            name = "Two"
        "#;
        let feels = Feels::pack(Pack::from_toml_str(src, "test").unwrap());
        assert_eq!(feels.designed_for(), "snappy");
        let (feel, warnings) = feels.feel("snappy").unwrap();
        assert_eq!(feel.roster.len(), 2);
        assert!(warnings.is_empty());
        let (_, warnings) = feels.feel("floaty").unwrap();
        assert_eq!(warnings.len(), 1, "not designed for it: {warnings:?}");
    }
}
