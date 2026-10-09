//! Install receipts: the manager's memory of what it installed.
//!
//! Each installed plugin carries a `.bitty-manager` file recording the
//! source it cloned from, the full commit hash installed, and the previous
//! hash (or `none`) for single-level rollback. The format is three strict
//! `key=value` lines; anything else is [`crate::ErrorKind::Receipt`] and
//! fails closed. Receipts are written after the checkout is verified and
//! before activation, so a receipt always describes bits on disk.

use std::path::{Path, PathBuf};

use crate::{Error, ErrorKind, GitRev, PluginSource};

/// Receipt file name inside an installed plugin directory.
pub const RECEIPT_FILE: &str = ".bitty-manager";

/// What the manager installed for one plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    source: PluginSource,
    rev: GitRev,
    prev: Option<GitRev>,
}

impl Receipt {
    /// Build a receipt for a fresh install (`prev` is none).
    pub fn fresh(source: &PluginSource, rev: &GitRev) -> Self {
        Self {
            source: source.clone(),
            rev: rev.clone(),
            prev: None,
        }
    }

    /// The source the plugin cloned from.
    pub fn source(&self) -> &PluginSource {
        &self.source
    }

    /// The installed full commit hash.
    pub fn rev(&self) -> &GitRev {
        &self.rev
    }

    /// The previous hash available for rollback, if any.
    pub fn prev(&self) -> Option<&GitRev> {
        self.prev.as_ref()
    }

    /// Record an update: the current rev becomes the rollback target.
    pub fn moved(&self, rev: &GitRev) -> Self {
        Self {
            source: self.source.clone(),
            rev: rev.clone(),
            prev: Some(self.rev.clone()),
        }
    }

    /// Record a rollback: the previous rev is installed, and the undo level
    /// is consumed (a second rollback fails until the next update).
    pub fn rolled_back(&self) -> Result<Self, Error> {
        let prev = self
            .prev
            .clone()
            .ok_or_else(|| Error::new(ErrorKind::State, "no previous revision to roll back to"))?;
        Ok(Self {
            source: self.source.clone(),
            rev: prev,
            prev: None,
        })
    }

    /// Receipt path for a plugin directory.
    pub fn path_for(plugin_dir: &Path) -> PathBuf {
        plugin_dir.join(RECEIPT_FILE)
    }

    /// Serialize to the on-disk format.
    pub fn render(&self) -> String {
        let prev = self.prev.as_ref().map(|r| r.as_str()).unwrap_or("none");
        format!(
            "repo={}\nrev={}\nprev={}\n",
            self.source.as_str(),
            self.rev.as_str(),
            prev
        )
    }

    /// Write the receipt into a plugin directory (verified checkout first).
    pub fn write_to(&self, plugin_dir: &Path) -> Result<(), Error> {
        use std::io::Write;
        let tmp = plugin_dir.join(format!("{RECEIPT_FILE}.tmp"));
        {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(self.render().as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp, Self::path_for(plugin_dir))?;
        Ok(())
    }

    /// Read and strictly validate a receipt. Unknown keys, missing keys,
    /// extra lines, or values that no longer validate all fail closed.
    pub fn read_from(plugin_dir: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(Self::path_for(plugin_dir)).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::new(
                    ErrorKind::Receipt,
                    "no install receipt: directory was not installed by this manager",
                )
            } else {
                Error::from(err)
            }
        })?;
        Self::parse(&text)
    }

    fn parse(text: &str) -> Result<Self, Error> {
        let corrupt = || {
            Error::new(
                ErrorKind::Receipt,
                "corrupt install receipt: expected repo/rev/prev lines",
            )
        };
        let mut repo: Option<&str> = None;
        let mut rev: Option<&str> = None;
        let mut prev: Option<&str> = None;
        let mut lines = 0;
        for line in text.lines() {
            lines += 1;
            let (key, value) = line.split_once('=').ok_or_else(corrupt)?;
            match key {
                "repo" if repo.is_none() => repo = Some(value),
                "rev" if rev.is_none() => rev = Some(value),
                "prev" if prev.is_none() => prev = Some(value),
                _ => return Err(corrupt()),
            }
        }
        if lines != 3 {
            return Err(corrupt());
        }
        let source = PluginSource::parse(repo.ok_or_else(corrupt)?).map_err(|_| corrupt())?;
        let rev = GitRev::parse(rev.ok_or_else(corrupt)?).map_err(|_| corrupt())?;
        let prev_raw = prev.ok_or_else(corrupt)?;
        let prev = if prev_raw == "none" {
            None
        } else {
            Some(GitRev::parse(prev_raw).map_err(|_| corrupt())?)
        };
        // Receipts always store full hashes: pins written by this manager
        // come from `rev-parse HEAD`.
        if rev.as_str().len() != 40 && rev.as_str().len() != 64 {
            return Err(corrupt());
        }
        if let Some(previous) = &prev {
            if previous.as_str().len() != 40 && previous.as_str().len() != 64 {
                return Err(corrupt());
            }
        }
        Ok(Self { source, rev, prev })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (PluginSource, GitRev) {
        let source = PluginSource::parse("https://github.com/bitty-terminal/palette")
            .expect("sample source");
        let rev = GitRev::parse("3a26417000000000000000000000000000000000").expect("sample rev");
        (source, rev)
    }

    #[test]
    fn round_trip() {
        let (source, rev) = sample();
        let receipt = Receipt::fresh(&source, &rev);
        assert_eq!(receipt.prev(), None);
        let back = Receipt::parse(&receipt.render()).expect("parses");
        assert_eq!(back, receipt);
        assert_eq!(back.source().kind(), crate::SourceKind::Https);
    }

    #[test]
    fn update_and_rollback_transitions() {
        let (source, rev) = sample();
        let fresh = Receipt::fresh(&source, &rev);
        assert!(fresh.rolled_back().is_err(), "fresh has no rollback");
        let next = GitRev::parse("ff00000000000000000000000000000000000000").expect("next rev");
        let moved = fresh.moved(&next);
        assert_eq!(moved.prev().expect("prev"), &rev);
        let back = moved.rolled_back().expect("rollback");
        assert_eq!(back.rev(), &rev, "rollback restores the old rev");
        assert_eq!(back.prev(), None, "rollback consumes the undo level");
        assert!(back.rolled_back().is_err(), "second rollback fails");
    }

    #[test]
    fn rejects_corrupt_receipts() {
        let (source, rev) = sample();
        let good = Receipt::fresh(&source, &rev).render();
        assert!(Receipt::parse(&good).is_ok());
        let mut bad_cases = vec![
            String::new(),
            "repo=x\n".to_string(),
            "repo=https://github.com/o/r\nrev=zzz\nprev=none\n".to_string(),
            "repo=https://github.com/o/r\nrev=3a26417\nprev=none\n".to_string(),
            "repo=../escape\nrev=3a26417000000000000000000000000000000000\nprev=none\n"
                .to_string(),
            "repo=https://github.com/o/r\nrev=3a26417000000000000000000000000000000000\nprev=none\nextra=1\n"
                .to_string(),
            "repo=https://github.com/o/r\nrev=3a26417000000000000000000000000000000000\n".to_string(),
            "rev=3a26417000000000000000000000000000000000\nprev=none\nrepo=https://github.com/o/r\n"
                .to_string(),
        ];
        // Reordered keys parse (order-insensitive) but duplicate keys fail.
        assert!(
            Receipt::parse(&bad_cases.pop().expect("reordered")).is_ok(),
            "key order is insignificant"
        );
        for bad in bad_cases {
            assert!(Receipt::parse(&bad).is_err(), "{bad:?} must be corrupt");
        }
        let dup = "repo=https://github.com/o/r\nrepo=https://github.com/o/r\nrev=3a26417000000000000000000000000000000000\n";
        assert!(Receipt::parse(dup).is_err(), "duplicate keys are corrupt");
    }

    #[test]
    fn missing_file_reports_not_managed() {
        let dir = std::env::temp_dir().join("bitty-manager-no-receipt-probe");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("probe dir");
        let err = Receipt::read_from(&dir).expect_err("no receipt");
        assert_eq!(err.kind(), ErrorKind::Receipt);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
