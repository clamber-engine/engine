//! Scenario and trace formats for Clamber conformance testing.
//!
//! Implements, as of spec-v0.3.0:
//!
//! - `spec/scenarios/FORMAT.md` ([`scenario`])
//! - `spec/traces/FORMAT.md` ([`trace`])
//! - `spec/traces/VOCABULARY.md` ([`vocab`])
//! - `spec/traces/METRICS.md` ([`metrics`], [`compare`])
//!
//! The engine recorder, the `clamber-trace` CLI and the test harness all
//! share this code, so a trace means the same thing everywhere.

/// Declares a closed set of names defined by the spec, with lookups both
/// ways. Names are never renamed or reused, so the list only grows.
macro_rules! names {
    (
        $(#[$meta:meta])*
        pub enum $ty:ident {
            $($(#[$vmeta:meta])* $variant:ident => $name:literal,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $ty {
            $($(#[$vmeta])* $variant,)*
        }

        impl $ty {
            /// Every name, in spec order.
            pub const ALL: &'static [$ty] = &[$($ty::$variant,)*];

            /// The name as written in spec files.
            pub fn name(self) -> &'static str {
                match self {
                    $($ty::$variant => $name,)*
                }
            }

            /// Looks up a name as written in spec files.
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some($ty::$variant),)*
                    _ => None,
                }
            }
        }

        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.name())
            }
        }
    };
}

pub mod compare;
pub mod decimal;
mod error;
pub mod input;
pub mod metrics;
pub mod report;
pub mod scenario;
pub mod trace;
pub mod vocab;

pub use compare::{Outcome, compare, title_mismatch};
pub use decimal::Decimal;
pub use error::ParseError;
pub use input::Buttons;
pub use metrics::{Comparison, Curve, Field, Metric};
pub use scenario::Scenario;
pub use trace::{Row, Trace};
pub use vocab::{Event, State};
