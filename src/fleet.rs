//! Downstream fleet: validated git checkouts under a plugins directory.
//!
//! Layout: `<dir>/<name>/` is a full git checkout on a detached HEAD at the
//! pinned revision, plus a `.bitty-manager` receipt. Operations:
//!
//! - [`Fleet::install`]: clone to a staging directory, check out the pinned
//!   revision, verify `HEAD`, write the receipt, then atomically rename into
//!   place. Never overwrites an existing entry.
//! - [`Fleet::update`]: fetch, check out the new revision, verify, and move
//!   the old revision into the receipt as the rollback target.
//! - [`Fleet::rollback`]: check out the receipt's previous revision and
//!   consume the undo level.
//! - [`Fleet::installed`]: read back what is installed, if anything.
//!
//! Fail-soft: install refuses to overwrite; update leaves the old checkout
//! and receipt untouched when the new revision cannot be fetched or checked
//! out; every hostile input is rejected during validation, before any spawn
//! (the fleet API takes only validated [`PluginName`], [`GitRev`], and
//! [`PluginSource`] values, so unvalidated strings cannot reach git).

use std::path::{Path, PathBuf};

use crate::git::GitRunner;
use crate::{Error, ErrorKind, GitRev, PluginName, PluginSource, Receipt, SystemGit};

/// Staging directory prefix inside the plugins root (suffixed with the name).
const STAGING_PREFIX: &str = ".staging-";

/// What is installed for one plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Plugin name.
    pub name: PluginName,
    /// Source it cloned from.
    pub source: PluginSource,
    /// Installed full commit hash.
    pub rev: GitRev,
    /// Previous hash available for rollback, if any.
    pub prev: Option<GitRev>,
}

/// Manager over one plugins directory.
#[derive(Debug)]
pub struct Fleet<G = SystemGit> {
    dir: PathBuf,
    git: G,
}

impl Fleet<SystemGit> {
    /// Open the manager over `dir` with the system git runner. The directory
    /// is created on first install; opening never creates anything.
    pub fn open(dir: PathBuf) -> Self {
        Self {
            dir,
            git: SystemGit::new(),
        }
    }
}

impl<G: GitRunner> Fleet<G> {
    /// Open with an explicit runner (tests inject a fake here).
    pub fn with_runner(dir: PathBuf, git: G) -> Self {
        Self { dir, git }
    }

    /// Directory this fleet manages.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn plugin_dir(&self, name: &PluginName) -> PathBuf {
        // `PluginName` cannot contain separators, `.`, or `..`, so joining
        // cannot escape `dir`. Checked once here as defense in depth.
        let path = self.dir.join(name.as_str());
        debug_assert!(path.starts_with(&self.dir));
        path
    }

    /// Read back what is installed for `name`, or `None` when absent.
    /// A present-but-unmanaged directory (or a corrupt receipt) is an error,
    /// never silently adopted.
    pub fn installed(&self, name: &PluginName) -> Result<Option<Installed>, Error> {
        let plugin_dir = self.plugin_dir(name);
        if !plugin_dir.exists() {
            return Ok(None);
        }
        let receipt = Receipt::read_from(&plugin_dir)?;
        Ok(Some(Installed {
            name: name.clone(),
            source: receipt.source().clone(),
            rev: receipt.rev().clone(),
            prev: receipt.prev().cloned(),
        }))
    }

    /// Install `name` from `source` at `rev`. Fails when the entry already
    /// exists or a previous staging directory was left behind.
    pub fn install(
        &self,
        name: &PluginName,
        source: &PluginSource,
        rev: &GitRev,
    ) -> Result<(), Error> {
        let plugin_dir = self.plugin_dir(name);
        if plugin_dir.exists() {
            return Err(Error::new(
                ErrorKind::State,
                format!("{name} is already installed; update or remove it first"),
            ));
        }
        let staging = self.dir.join(format!("{STAGING_PREFIX}{name}"));
        if staging.exists() {
            return Err(Error::new(
                ErrorKind::State,
                format!("stale staging directory for {name}; remove it and retry"),
            ));
        }
        std::fs::create_dir_all(&self.dir)?;
        let cleanup = StagingGuard {
            path: staging.clone(),
        };
        let staging_arg = staging_name(name);
        self.git.run(
            &["clone", "--", source.as_str(), staging_arg.as_str()],
            &self.dir,
        )?;
        self.checkout_and_verify(&staging, rev)?;
        let head = self.head(&staging)?;
        Receipt::fresh(source, &head).write_to(&staging)?;
        std::fs::rename(&staging, &plugin_dir)?;
        cleanup.disarm();
        Ok(())
    }

    /// Move an installed plugin to `rev`, keeping the old revision for
    /// [`Fleet::rollback`]. The old checkout and receipt stay in place until
    /// the new revision is fetched, checked out, and verified.
    pub fn update(&self, name: &PluginName, rev: &GitRev) -> Result<(), Error> {
        let plugin_dir = self.plugin_dir(name);
        let receipt = Receipt::read_from(&plugin_dir)?;
        let old = receipt.rev().clone();
        if rev.matches_full(old.as_str()) {
            return Err(Error::new(
                ErrorKind::State,
                format!("{name} is already at {rev}"),
            ));
        }
        self.git.run(&["fetch", "origin"], &plugin_dir)?;
        if let Err(err) = self.checkout_and_verify(&plugin_dir, rev) {
            // Best effort: leave the tree where it was before reporting.
            let _ = self.git.run(&["checkout", old.as_str()], &plugin_dir);
            return Err(err);
        }
        let head = self.head(&plugin_dir)?;
        if let Err(err) = receipt.moved(&head).write_to(&plugin_dir) {
            // The tree is ahead of the receipt; restore the old checkout
            // best-effort so the receipt describes the bits again.
            let _ = self.git.run(&["checkout", old.as_str()], &plugin_dir);
            return Err(Error::new(
                ErrorKind::Io,
                format!(
                    "update of {name} failed to record {head}: {err}; restored {old} best-effort"
                ),
            ));
        }
        Ok(())
    }

    /// Restore the revision saved by the last [`Fleet::update`]. Consumes
    /// the undo level: a second rollback fails until the next update.
    pub fn rollback(&self, name: &PluginName) -> Result<(), Error> {
        let plugin_dir = self.plugin_dir(name);
        let receipt = Receipt::read_from(&plugin_dir)?;
        let back = receipt.rolled_back()?;
        self.checkout_and_verify(&plugin_dir, back.rev())?;
        let head = self.head(&plugin_dir)?;
        if !back.rev().matches_full(head.as_str()) {
            return Err(Error::new(
                ErrorKind::Git,
                format!("rollback of {name} verified against an unexpected HEAD"),
            ));
        }
        back.write_to(&plugin_dir)?;
        Ok(())
    }

    fn checkout_and_verify(&self, cwd: &Path, rev: &GitRev) -> Result<(), Error> {
        self.git.run(&["checkout", rev.as_str()], cwd)?;
        let head = self.head(cwd)?;
        if !rev.matches_full(head.as_str()) {
            return Err(Error::new(
                ErrorKind::Git,
                "checkout landed on an unexpected commit".to_string(),
            ));
        }
        Ok(())
    }

    fn head(&self, cwd: &Path) -> Result<GitRev, Error> {
        let out = self.git.run(&["rev-parse", "--verify", "HEAD"], cwd)?;
        GitRev::parse(out.stdout.trim()).map_err(|_| {
            Error::new(
                ErrorKind::Git,
                "git reported an unparsable HEAD".to_string(),
            )
        })
    }
}

/// Best-effort removal of our own staging directory on failure. Only ever
/// points at `<dir>/.staging-<validated-name>`, which this install created.
struct StagingGuard {
    path: PathBuf,
}

impl StagingGuard {
    fn disarm(self) {
        std::mem::forget(self);
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn staging_name(name: &PluginName) -> String {
    format!("{STAGING_PREFIX}{name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GitOutput;
    use crate::git::fake::FakeGit;

    const FULL_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const FULL_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn tmp_root(case: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bitty-manager-{case}"))
    }

    fn name() -> PluginName {
        PluginName::parse("palette").expect("name")
    }

    fn source() -> PluginSource {
        PluginSource::parse("https://github.com/bitty-terminal/palette").expect("source")
    }

    fn script_head(fake: &FakeGit, full: &str) {
        // clone/fetch, checkout, verify rev-parse, final rev-parse.
        fake.push_stdout("");
        fake.push_stdout("");
        fake.push_stdout(full);
        fake.push_stdout(full);
    }

    /// Test-only runner: replays [`FakeGit`] scripts and adds the one
    /// filesystem effect a real `git clone` has (creating the target
    /// directory), which the pure fake deliberately lacks.
    struct CloneMakingGit<'a> {
        inner: &'a FakeGit,
    }

    impl GitRunner for CloneMakingGit<'_> {
        fn run(&self, argv: &[&str], cwd: &std::path::Path) -> Result<GitOutput, Error> {
            if argv.first() == Some(&"clone") {
                let dst = argv.last().expect("clone target");
                std::fs::create_dir_all(cwd.join(dst)).expect("fake clone effect");
            }
            self.inner.run(argv, cwd)
        }
    }

    #[test]
    fn install_verifies_then_activates() {
        let dir = tmp_root("install-ok");
        let _ = std::fs::remove_dir_all(&dir);
        let fake = FakeGit::new();
        script_head(&fake, FULL_A);
        let runner = CloneMakingGit { inner: &fake };
        let fleet = Fleet::with_runner(dir.clone(), runner);
        let rev = GitRev::parse(&FULL_A[..7]).expect("rev");
        fleet.install(&name(), &source(), &rev).expect("install");
        let installed = fleet
            .installed(&name())
            .expect("read back")
            .expect("present");
        assert_eq!(installed.rev.as_str(), FULL_A);
        assert_eq!(installed.prev, None);
        let log = fake.argv_log();
        assert_eq!(log.len(), 4, "clone, checkout, verify, rev-parse only");
        assert_eq!(
            log[0][..3],
            ["clone", "--", "https://github.com/bitty-terminal/palette"]
        );
        assert!(log[0][3].starts_with(".staging-"), "clone lands in staging");
        assert!(
            !dir.join(".staging-palette").exists(),
            "staging renamed away"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_refuses_existing_and_stale_staging() {
        let dir = tmp_root("install-refuse");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("palette")).expect("existing entry");
        std::fs::create_dir_all(dir.join(".staging-other")).expect("stale staging");
        let fake = FakeGit::new();
        let fleet = Fleet::with_runner(dir.clone(), fake);
        let rev = GitRev::parse(&FULL_A[..7]).expect("rev");
        let err = fleet
            .install(&name(), &source(), &rev)
            .expect_err("existing entry");
        assert_eq!(err.kind(), ErrorKind::State);
        let other = PluginName::parse("other").expect("other");
        let stale = fleet
            .install(&other, &source(), &rev)
            .expect_err("stale staging");
        assert_eq!(stale.kind(), ErrorKind::State);
        assert_eq!(fleet.git.spawns(), 0, "refusals happen before any spawn");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_moves_rev_and_keeps_rollback() {
        let dir = tmp_root("update-ok");
        let _ = std::fs::remove_dir_all(&dir);
        let plugin_dir = dir.join("palette");
        std::fs::create_dir_all(&plugin_dir).expect("entry");
        Receipt::fresh(&source(), &GitRev::parse(FULL_A).expect("a"))
            .write_to(&plugin_dir)
            .expect("receipt");
        let fake = FakeGit::new();
        script_head(&fake, FULL_B);
        let fleet = Fleet::with_runner(dir.clone(), fake);
        fleet
            .update(&name(), &GitRev::parse(&FULL_B[..7]).expect("b"))
            .expect("update");
        let installed = fleet
            .installed(&name())
            .expect("read back")
            .expect("present");
        assert_eq!(installed.rev.as_str(), FULL_B);
        assert_eq!(
            installed.prev.as_ref().map(|r| r.as_str()),
            Some(FULL_A),
            "old rev kept for rollback"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_failure_keeps_old_checkout() {
        let dir = tmp_root("update-fail");
        let _ = std::fs::remove_dir_all(&dir);
        let plugin_dir = dir.join("palette");
        std::fs::create_dir_all(&plugin_dir).expect("entry");
        Receipt::fresh(&source(), &GitRev::parse(FULL_A).expect("a"))
            .write_to(&plugin_dir)
            .expect("receipt");
        let fake = FakeGit::new();
        // fetch, checkout, verify rev-parse, then the restore checkout.
        fake.push_stdout("");
        fake.push_output(Err(Error::new(ErrorKind::Git, "checkout failed")));
        fake.push_stdout("");
        let fleet = Fleet::with_runner(dir.clone(), fake);
        let err = fleet
            .update(&name(), &GitRev::parse(&FULL_B[..7]).expect("b"))
            .expect_err("bad checkout");
        assert_eq!(err.kind(), ErrorKind::Git);
        let installed = fleet
            .installed(&name())
            .expect("read back")
            .expect("present");
        assert_eq!(
            installed.rev.as_str(),
            FULL_A,
            "receipt still points at old"
        );
        assert_eq!(installed.prev, None, "no rollback level invented");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_to_current_is_a_no_op_without_spawn() {
        let dir = tmp_root("update-current");
        let _ = std::fs::remove_dir_all(&dir);
        let plugin_dir = dir.join("palette");
        std::fs::create_dir_all(&plugin_dir).expect("entry");
        Receipt::fresh(&source(), &GitRev::parse(FULL_A).expect("a"))
            .write_to(&plugin_dir)
            .expect("receipt");
        let fake = FakeGit::new();
        let fleet = Fleet::with_runner(dir.clone(), fake);
        // Full and abbreviated spellings of the installed commit both refuse.
        for rev in [FULL_A, &FULL_A[..7]] {
            let err = fleet
                .update(&name(), &GitRev::parse(rev).expect("rev"))
                .expect_err("already current");
            assert_eq!(err.kind(), ErrorKind::State);
        }
        assert_eq!(fleet.git.spawns(), 0, "no fetch for a no-op update");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zero_spawn_on_hostile_inputs() {
        // Hostile values never construct, so the fleet API cannot even be
        // called with them.
        for bad_name in ["../x", "a/b", "-lead", "UPPER", ""] {
            assert!(PluginName::parse(bad_name).is_err());
        }
        for bad_rev in ["HEAD", "main", "--upload-pack=x", "12"] {
            assert!(GitRev::parse(bad_rev).is_err());
        }
        for bad_source in [
            "http://example.com/o/r",
            "ssh://git@example.com/o/r",
            "ext::sh -c whoami",
            "../escape",
        ] {
            assert!(PluginSource::parse(bad_source).is_err());
        }
    }
}
