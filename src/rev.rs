//! Validated git revisions. Registry pins are commit hashes (full or
//! abbreviated), never branch names or ref expressions: only ASCII hex of
//! sha1-short length or more is accepted, so refspec smuggling (`HEAD`,
//! `HEAD@{1}`, `-u<hook>`, option-like values) is unrepresentable.

use crate::{Error, ErrorKind};

/// Minimum accepted length: a 7-character abbreviated SHA, the shortest git
/// accepts for unambiguous lookup in practice.
pub const MIN_REV_LEN: usize = 7;
/// Maximum accepted length: a 64-character SHA-256 hex digest.
pub const MAX_REV_LEN: usize = 64;

/// A validated commit hash, stored lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitRev(String);

impl GitRev {
    /// Validate `input`: 7..=64 ASCII hex characters. Uppercase hex is
    /// accepted and normalized to lowercase.
    pub fn parse(input: &str) -> Result<Self, Error> {
        if input.len() < MIN_REV_LEN || input.len() > MAX_REV_LEN {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "invalid git revision: length must be {MIN_REV_LEN}..={MAX_REV_LEN} hex characters"
                ),
            ));
        }
        if !input.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid git revision: only hexadecimal characters",
            ));
        }
        Ok(Self(input.to_ascii_lowercase()))
    }

    /// The validated (lowercase) revision.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `full` (a full object hash from `rev-parse`) designates this
    /// revision: equality for full hashes, prefix match for abbreviations.
    pub fn matches_full(&self, full: &str) -> bool {
        full.starts_with(&self.0)
    }
}

impl std::fmt::Display for GitRev {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_hashes() {
        for valid in [
            "3a26417",
            "e1723b60cc94d3abc18821c9e6b14c6c88f33add",
            "9AEF93979999999999999999999999999999999999",
            "abababababababababababababababababababababababababababababababab",
        ] {
            let rev = GitRev::parse(valid);
            assert!(rev.is_ok(), "{valid} should parse");
        }
        let rev = GitRev::parse("ABCDEF1").expect("uppercase parses");
        assert_eq!(rev.as_str(), "abcdef1", "uppercase normalizes");
    }

    #[test]
    fn rejects_hostile_revs() {
        let hostile = [
            "",
            "123456",
            &"a".repeat(MAX_REV_LEN + 1),
            "HEAD",
            "main",
            "HEAD@{1}",
            "HEAD~3",
            ":/fixup",
            "-uploader",
            "--upload-pack=x",
            "zzzzzzz",
            "abc def1",
            "abcdef\n1",
            "abcdef1\x00",
            "@{u}",
            "refs/heads/main",
            "12:34;56",
        ];
        for input in hostile {
            assert!(GitRev::parse(input).is_err(), "{input:?} must be rejected");
        }
    }

    #[test]
    fn prefix_matching() {
        let short = GitRev::parse("3a26417").expect("short parses");
        assert!(short.matches_full("3a264170000000000000000000000000000000000"));
        assert!(!short.matches_full("ff00000000000000000000000000000000000000"));
        let full = GitRev::parse("3a264170000000000000000000000000000000000").expect("full parses");
        assert!(full.matches_full("3a264170000000000000000000000000000000000"));
        assert!(!full.matches_full("3a264170000000000000000000000000000000001"));
    }
}
