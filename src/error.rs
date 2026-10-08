//! Typed errors for the manager. Every fallible operation returns [`Error`];
//! hostile inputs and I/O, git, and receipt failures are distinguished by
//! [`ErrorKind`] so callers (and the CLI) can fail soft with a clear message.

use std::fmt;

/// Machine-readable failure class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A plugin name, git revision, or source failed validation. Returned
    /// before any process is spawned.
    InvalidInput,
    /// A git subprocess failed or behaved unexpectedly.
    Git,
    /// Filesystem I/O outside git failed.
    Io,
    /// A plugin directory is missing or already present.
    State,
    /// An install receipt is missing, malformed, or inconsistent.
    Receipt,
}

/// Manager failure: a kind plus a human-readable message. Messages never
/// echo more than the offending value class (no secrets exist in this
/// slice; hostile inputs are quoted only by kind, not content, when they
/// could carry control bytes — see [`Error::message`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    /// Build an error. `message` must already be sanitized by the caller
    /// (validated values only; hostile raw input is never interpolated).
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Machine-readable failure class.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, format!("filesystem error: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_shows_message() {
        let err = Error::new(ErrorKind::State, "already installed");
        assert_eq!(format!("{err}"), "already installed");
        assert_eq!(err.kind(), ErrorKind::State);
    }

    #[test]
    fn io_converts() {
        let err = Error::from(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
        assert_eq!(err.kind(), ErrorKind::Io);
    }
}
