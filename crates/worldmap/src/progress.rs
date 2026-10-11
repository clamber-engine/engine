//! Progress through the world: what's been cleared, found and revealed.
//! Part of save data.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Everything the world map needs to remember between sessions. Places are
/// written `map:node`, so progress spans nested maps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    /// Levels cleared, with the exits they were cleared by.
    #[serde(default)]
    pub cleared: BTreeMap<String, BTreeSet<String>>,
    /// Story flags set by events.
    #[serde(default)]
    pub flags: BTreeSet<String>,
    /// Counts of collected things.
    #[serde(default)]
    pub counters: BTreeMap<String, u32>,
    /// Hidden nodes and paths already shown to the player, so a reveal
    /// plays once.
    #[serde(default)]
    pub revealed: BTreeSet<String>,
    /// Where the player is.
    pub position: Option<Position>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub map: String,
    /// The node the player stands on, on a path map.
    pub node: Option<String>,
    /// Where the player is on a free-roaming map, as fixed-point bits so
    /// it restores exactly.
    pub at: Option<[i64; 2]>,
}

impl Progress {
    /// Records clearing level `place` (`map:node`) by `exit`.
    pub fn clear(&mut self, place: &str, exit: &str) {
        self.cleared
            .entry(place.to_string())
            .or_default()
            .insert(exit.to_string());
    }

    pub fn is_cleared(&self, place: &str) -> bool {
        self.cleared.contains_key(place)
    }

    pub fn cleared_by(&self, place: &str, exit: &str) -> bool {
        self.cleared.get(place).is_some_and(|e| e.contains(exit))
    }

    pub fn counter(&self, name: &str) -> u32 {
        self.counters.get(name).copied().unwrap_or(0)
    }

    pub fn add(&mut self, counter: &str, n: u32) {
        let c = self.counters.entry(counter.to_string()).or_default();
        *c = c.saturating_add(n);
    }
}
