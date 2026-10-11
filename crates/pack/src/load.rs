//! Reading content files: errors, and the value conventions shared by
//! packs, presets and (later) levels.

use std::collections::BTreeMap;
use std::fmt;

use clamber_fixed::Fixed;
use serde::de::DeserializeOwned;
use toml::{Table, Value};

use crate::physics::{Adjust, Param, ParamsPatch, Patch, PhysicsPatch};

/// A content file that couldn't be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    /// Where the text came from: a file path, or a name given by the caller.
    pub origin: String,
    /// What's wrong, starting with the key path when there is one.
    pub message: String,
}

impl LoadError {
    pub(crate) fn new(origin: &str, message: impl Into<String>) -> Self {
        LoadError {
            origin: origin.to_string(),
            message: message.into(),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for LoadError {}

/// Context for reading one file: errors name the file and key path.
pub(crate) struct Reader<'a> {
    pub origin: &'a str,
}

impl Reader<'_> {
    pub fn err(&self, path: &str, message: impl fmt::Display) -> LoadError {
        LoadError::new(self.origin, format!("{path}: {message}"))
    }

    pub fn parse_document<T: DeserializeOwned>(&self, src: &str) -> Result<T, LoadError> {
        toml::from_str(src).map_err(|e| LoadError::new(self.origin, e.to_string().trim_end()))
    }

    pub fn structure<T: DeserializeOwned>(
        &self,
        path: &str,
        value: &Value,
    ) -> Result<T, LoadError> {
        value
            .clone()
            .try_into()
            .map_err(|e: toml::de::Error| self.err(path, e.message()))
    }

    pub fn table<'v>(&self, path: &str, value: &'v Value) -> Result<&'v Table, LoadError> {
        value
            .as_table()
            .ok_or_else(|| self.err(path, "expected a table"))
    }

    /// A number: an integer or a float, stored exactly when it's a binary
    /// fraction and rounded to the nearest 1/2^32 otherwise.
    pub fn number(&self, path: &str, value: &Value) -> Result<Fixed, LoadError> {
        let fixed = match *value {
            Value::Integer(n) => i32::try_from(n).ok().map(Fixed::from_int),
            Value::Float(x) => Fixed::from_f64(x),
            _ => return Err(self.err(path, "expected a number")),
        };
        fixed.ok_or_else(|| self.err(path, "number out of range"))
    }

    /// A plain number sets a value; `{ scale = <factor> }` multiplies it.
    pub fn adjust(&self, path: &str, value: &Value) -> Result<Adjust, LoadError> {
        let Value::Table(table) = value else {
            return self.number(path, value).map(Adjust::Set);
        };
        let shape = || self.err(path, "expected a number or `{ scale = <factor> }`");
        if table.len() != 1 {
            return Err(shape());
        }
        let factor = table.get("scale").ok_or_else(shape)?;
        let factor = self.number(&format!("{path}.scale"), factor)?;
        if factor.is_negative() {
            return Err(self.err(path, "scale factor must not be negative"));
        }
        Ok(Adjust::Scale(factor))
    }

    /// A table of movement parameters to adjust.
    pub fn physics_patch(&self, path: &str, table: &Table) -> Result<PhysicsPatch, LoadError> {
        let mut patch = BTreeMap::new();
        for (key, value) in table {
            let at = format!("{path}.{key}");
            let param =
                Param::from_name(key).ok_or_else(|| self.err(&at, "unknown movement parameter"))?;
            let adjust = self.adjust(&at, value)?;
            if let Adjust::Set(v) = adjust
                && v.is_negative()
                && !param.allows_negative()
            {
                return Err(self.err(&at, "must not be negative"));
            }
            patch.insert(param, adjust);
        }
        Ok(Patch(patch))
    }

    /// A table of ability parameters to adjust.
    pub fn params_patch(&self, path: &str, table: &Table) -> Result<ParamsPatch, LoadError> {
        let mut patch = BTreeMap::new();
        for (key, value) in table {
            let at = format!("{path}.{key}");
            self.name(&at, key)?;
            patch.insert(key.clone(), self.adjust(&at, value)?);
        }
        Ok(Patch(patch))
    }

    /// A table of ability parameters with plain values.
    pub fn params(&self, path: &str, table: &Table) -> Result<BTreeMap<String, Fixed>, LoadError> {
        let mut params = BTreeMap::new();
        for (key, value) in table {
            let at = format!("{path}.{key}");
            self.name(&at, key)?;
            params.insert(key.clone(), self.number(&at, value)?);
        }
        Ok(params)
    }

    /// An id for a pack, character or preset: lowercase letters, digits,
    /// `-` and `_`, starting with a letter or digit.
    pub fn id(&self, path: &str, id: &str) -> Result<(), LoadError> {
        let mut bytes = id.bytes();
        let ok = bytes
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            && bytes
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if ok {
            Ok(())
        } else {
            Err(self.err(
                path,
                format!("`{id}` is not a valid id (use a-z, 0-9, - and _)"),
            ))
        }
    }

    /// A name for an ability or ability parameter: lowercase letters,
    /// digits and `_`, starting with a letter.
    pub fn name(&self, path: &str, name: &str) -> Result<(), LoadError> {
        let mut bytes = name.bytes();
        let ok = bytes.next().is_some_and(|b| b.is_ascii_lowercase())
            && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if ok {
            Ok(())
        } else {
            Err(self.err(
                path,
                format!("`{name}` is not a valid name (use a-z, 0-9 and _)"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Reader = Reader { origin: "test" };

    fn value(src: &str) -> Value {
        let table: Table = toml::from_str(&format!("v = {src}")).unwrap();
        table["v"].clone()
    }

    #[test]
    fn numbers() {
        assert_eq!(R.number("v", &value("3")), Ok(Fixed::from_int(3)));
        assert_eq!(
            R.number("v", &value("-0.5")),
            Ok(Fixed::from_ratio_pow2(-1, 1))
        );
        for bad in ["\"1\"", "true", "nan", "inf", "3000000000", "1e10"] {
            assert!(R.number("v", &value(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn adjustments() {
        assert_eq!(
            R.adjust("v", &value("2")),
            Ok(Adjust::Set(Fixed::from_int(2)))
        );
        assert_eq!(
            R.adjust("v", &value("{ scale = 0.5 }")),
            Ok(Adjust::Scale(Fixed::from_ratio_pow2(1, 1)))
        );
        for bad in [
            "{}",
            "{ scale = -1 }",
            "{ factor = 2 }",
            "{ scale = 1, set = 2 }",
        ] {
            assert!(R.adjust("v", &value(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn physics_patches_check_names_and_signs() {
        let table = |src: &str| -> Table { toml::from_str(src).unwrap() };
        assert!(R.physics_patch("w", &table("gravity = -0.25")).is_ok());
        let err = R.physics_patch("w", &table("top_speed = -1")).unwrap_err();
        assert_eq!(err.message, "w.top_speed: must not be negative");
        let err = R.physics_patch("w", &table("topspeed = 1")).unwrap_err();
        assert_eq!(err.message, "w.topspeed: unknown movement parameter");
    }

    #[test]
    fn ids_and_names() {
        for good in ["default", "classic-1", "a_b", "9lives"] {
            assert!(R.id("id", good).is_ok(), "{good}");
        }
        for bad in ["", "-x", "Upper", "a b", "é"] {
            assert!(R.id("id", bad).is_err(), "{bad}");
        }
        assert!(R.name("n", "fall_speed").is_ok());
        for bad in ["9x", "fall-speed", "_x", ""] {
            assert!(R.name("n", bad).is_err(), "{bad}");
        }
    }
}
