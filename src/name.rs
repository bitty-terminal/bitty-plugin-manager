//! Validated plugin names. A name doubles as a directory entry under the
//! plugins root, so the charset is chosen to make path traversal and option
//! injection unrepresentable: lowercase ASCII letters, digits, and interior
//! hyphens only. Anything else is [`crate::Error`] of kind
//! [`crate::ErrorKind::InvalidInput`], returned before any spawn.

use crate::{Error, ErrorKind};

/// Maximum name length in bytes (names are ASCII, so bytes and chars agree).
pub const MAX_NAME_LEN: usize = 64;

/// A validated plugin name (e.g. `palette`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginName(String);

impl PluginName {
    /// Validate `input`. Rejects empty input, input over [`MAX_NAME_LEN`]
    /// bytes, and anything outside `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$`.
    pub fn parse(input: &str) -> Result<Self, Error> {
        if input.is_empty() || input.len() > MAX_NAME_LEN {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("invalid plugin name: length must be 1..={MAX_NAME_LEN} bytes"),
            ));
        }
        let bytes = input.as_bytes();
        let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        if !edge(bytes[0]) || !edge(bytes[bytes.len() - 1]) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid plugin name: must start and end with a lowercase letter or digit",
            ));
        }
        if !bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid plugin name: only lowercase letters, digits, and hyphens",
            ));
        }
        Ok(Self(input.to_string()))
    }

    /// The validated name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_well_formed_names() {
        for valid in ["a", "palette", "git-panel", "a1", "1a", "x-9-y"] {
            assert!(PluginName::parse(valid).is_ok(), "{valid} should parse");
        }
        let max = "a".repeat(MAX_NAME_LEN);
        assert!(PluginName::parse(&max).is_ok(), "64 bytes should parse");
    }

    #[test]
    fn rejects_hostile_names() {
        // Traversal, separators, injection, case, length, and edges.
        let hostile = [
            "",
            ".",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "-lead",
            "trail-",
            "-",
            "UPPER",
            "under_score",
            "with space",
            "dot.name",
            "semi;colon",
            "pipe|name",
            "dollar$name",
            "back`tick",
            "quote\"name",
            "apos'name",
            "paren(name)",
            "glob*name",
            "quest?name",
            "hash#name",
            "tilde~",
            "contr\x00ol",
            "newline\nname",
            "ünicode",
            "emoji🎉",
        ];
        for input in hostile {
            assert!(
                PluginName::parse(input).is_err(),
                "{input:?} must be rejected",
            );
        }
        let long = "a".repeat(MAX_NAME_LEN + 1);
        assert!(
            PluginName::parse(&long).is_err(),
            "65 bytes must be rejected"
        );
    }
}
