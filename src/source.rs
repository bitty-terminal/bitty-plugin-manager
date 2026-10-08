//! Validated plugin sources: where a checkout clones from.
//!
//! Two forms are accepted, and both are validated before any spawn:
//!
//! - `https://host/path` remotes (the registry form). The scheme must be
//!   lowercase `https`, the authority must be a bare hostname (no userinfo,
//!   no port), and the path must be a plain repository path (no query,
//!   fragment, whitespace, or backslashes). Anything else — `http`, `ssh`,
//!   `git@`, `file:`, `ext::`, option-like values — is rejected.
//! - Absolute local paths (air-gapped installs and tests). Must be absolute
//!   with no `..` segments, so a validated source can never escape the
//!   intended parent when joined or cloned.
//!
//! Relative paths are rejected outright: they resolve against the caller's
//! working directory, which an installer must never depend on.

use std::path::{Component, Path, PathBuf};

use crate::{Error, ErrorKind};

/// Maximum source length in bytes.
pub const MAX_SOURCE_LEN: usize = 1024;

/// Which validated form a source takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// `https://host/path` remote.
    Https,
    /// Absolute local path.
    LocalPath,
}

/// A validated clone source, kept in its original spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSource {
    raw: String,
    kind: SourceKind,
}

impl PluginSource {
    /// Validate `input` as an https remote or an absolute local path.
    pub fn parse(input: &str) -> Result<Self, Error> {
        if input.is_empty() || input.len() > MAX_SOURCE_LEN {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("invalid plugin source: length must be 1..={MAX_SOURCE_LEN} bytes"),
            ));
        }
        if input.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid plugin source: control characters are not allowed",
            ));
        }
        if input.contains("://") {
            Self::parse_remote(input)
        } else {
            Self::parse_local(input)
        }
    }

    fn parse_remote(input: &str) -> Result<Self, Error> {
        let invalid = || {
            Error::new(
                ErrorKind::InvalidInput,
                "invalid plugin source: https remotes must look like https://host/path",
            )
        };
        let after_scheme = input.strip_prefix("https://").ok_or_else(invalid)?;
        let slash = after_scheme.find('/').ok_or_else(invalid)?;
        let (authority, path) = after_scheme.split_at(slash);
        if authority.is_empty() || path.len() < 2 {
            return Err(invalid());
        }
        if authority.contains('@') || authority.contains(':') {
            return Err(invalid());
        }
        if !authority
            .split('.')
            .all(|label| !label.is_empty() && label.len() <= 63 && valid_label(label))
        {
            return Err(invalid());
        }
        if path.contains(['?', '#', ' ', '\\']) {
            return Err(invalid());
        }
        Ok(Self {
            raw: input.to_string(),
            kind: SourceKind::Https,
        })
    }

    fn parse_local(input: &str) -> Result<Self, Error> {
        let path = Path::new(input);
        if !path.is_absolute() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid plugin source: local sources must be absolute paths",
            ));
        }
        if path.components().any(|c| c == Component::ParentDir) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid plugin source: local paths must not contain ..",
            ));
        }
        Ok(Self {
            raw: input.to_string(),
            kind: SourceKind::LocalPath,
        })
    }

    /// Which validated form this source takes.
    pub fn kind(&self) -> SourceKind {
        self.kind
    }

    /// The validated source in its original spelling, for use as a single
    /// argv element (never interpolated into a shell).
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The validated source as a filesystem path (local sources only).
    pub fn as_path(&self) -> Option<PathBuf> {
        match self.kind {
            SourceKind::LocalPath => Some(PathBuf::from(&self.raw)),
            SourceKind::Https => None,
        }
    }
}

impl std::fmt::Display for PluginSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

fn valid_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    let edge = |b: u8| b.is_ascii_alphanumeric();
    edge(bytes[0])
        && edge(bytes[bytes.len() - 1])
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_registry_remotes() {
        for valid in [
            "https://github.com/bitty-terminal/palette",
            "https://github.com/bitty-terminal/palette.git",
            "https://gitea.lan/o/r",
            "https://host/x",
        ] {
            let source = PluginSource::parse(valid);
            assert!(source.is_ok(), "{valid} should parse");
            assert_eq!(source.expect("parses").kind(), SourceKind::Https);
        }
    }

    #[test]
    fn accepts_absolute_local_paths() {
        for valid in ["/srv/git/palette", "/tmp/bitty-fixture"] {
            let source = PluginSource::parse(valid);
            assert!(source.is_ok(), "{valid} should parse");
            assert_eq!(source.expect("parses").kind(), SourceKind::LocalPath);
        }
    }

    #[test]
    fn rejects_hostile_sources() {
        let hostile = [
            "",
            "http://github.com/o/r",
            "HTTPS://github.com/o/r",
            "ssh://git@github.com/o/r",
            "git@github.com:o/r.git",
            "file:///srv/git/r",
            "ext::sh -c whoami",
            "https://user:pass@github.com/o/r",
            "https://github.com:443/o/r",
            "https://github.com",
            "https://github.com/",
            "https:///o/r",
            "https://-lead.example/o/r",
            "https://trail-.example/o/r",
            "https://exa mple.com/o/r",
            "https://example.com/o/r?x=1",
            "https://example.com/o/r#frag",
            "https://example.com\\o\\r",
            "--upload-pack=evil",
            "-uploader",
            "relative/path",
            "./relative",
            "../escape",
            "/abs/../escape",
            "https://example.com/o/r\ninjected",
            "/tmp/has\x00nul",
        ];
        for input in hostile {
            assert!(
                PluginSource::parse(input).is_err(),
                "{input:?} must be rejected",
            );
        }
        // The port-bearing remote from the acceptance test is invalid.
        assert!(PluginSource::parse("https://git.example-work-station01.internal:0/x").is_err());
    }
}
