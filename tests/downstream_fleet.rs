//! Integration suite for the downstream git-checkout fleet (issue #9).
//!
//! Uses the real system git binary against fixture repositories under a
//! temporary root: no network, no registry, no cargo on the target side.
//! Each case owns a unique directory (`bpm-it-<pid>-<case>`) removed
//! best-effort afterwards; leftovers on failure are intentional debugging
//! material, never user data.
//!
//! Properties proved here (unit tests in `src/` prove validation and the
//! fake-runner scripts):
//!
//! - install / update / rollback lifecycle with real checkouts, including
//!   abbreviated-rev installs and fetch-after-install updates;
//! - repository hooks planted in an installed clone never execute during
//!   update or rollback (the `-c core.hooksPath=/dev/null` control);
//! - hostile CLI inputs exit non-zero with zero side effects (no checkouts,
//!   no staging leftovers);
//! - foreign directories and corrupt receipts fail closed; rollback without
//!   a previous revision fails; update to the current revision is a no-op.

use std::path::{Path, PathBuf};
use std::process::Command;

use bitty_plugin_manager::{Fleet, GitRev, PluginName, PluginSource};

fn git(cwd: &Path, argv: &[&str]) -> String {
    let output = Command::new("git")
        .args(argv)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "bitty-test")
        .env("GIT_AUTHOR_EMAIL", "bitty-test@example.com")
        .env("GIT_COMMITTER_NAME", "bitty-test")
        .env("GIT_COMMITTER_EMAIL", "bitty-test@example.com")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {argv:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8")
        .trim()
        .to_string()
}

/// Unique scratch root for one case.
struct Case {
    root: PathBuf,
}

impl Case {
    fn new(case: &str) -> Self {
        let root = std::env::temp_dir().join(format!("bpm-it-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch root");
        Self { root }
    }

    fn plugins_dir(&self) -> PathBuf {
        self.root.join("plugins")
    }

    fn fleet(&self) -> Fleet {
        Fleet::open(self.plugins_dir())
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Fixture origin repo with one commit; returns (repo path, rev1).
fn fixture(case: &Case, name: &str, content: &str) -> (PathBuf, String) {
    let repo = case.root.join(name);
    std::fs::create_dir_all(&repo).expect("fixture dir");
    git(&repo, &["init", "-b", "main"]);
    std::fs::write(repo.join("plugin.lua"), content).expect("fixture file");
    git(&repo, &["add", "plugin.lua"]);
    git(&repo, &["commit", "-m", "rev1"]);
    let rev1 = git(&repo, &["rev-parse", "HEAD"]);
    (repo, rev1)
}

fn commit_more(repo: &Path, content: &str, message: &str) -> String {
    std::fs::write(repo.join("plugin.lua"), content).expect("fixture file");
    git(repo, &["add", "plugin.lua"]);
    git(repo, &["commit", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

fn name() -> PluginName {
    PluginName::parse("palette").expect("name")
}

fn source(repo: &Path) -> PluginSource {
    PluginSource::parse(repo.to_str().expect("utf8 path")).expect("source")
}

fn installed_rev(case: &Case) -> String {
    case.fleet()
        .installed(&name())
        .expect("read")
        .expect("installed")
        .rev
        .as_str()
        .to_string()
}

#[test]
fn install_update_rollback_lifecycle() {
    let case = Case::new("lifecycle");
    let (repo, rev1) = fixture(&case, "origin", "version = 1\n");
    let fleet = case.fleet();
    let from = source(&repo);

    // Install with an abbreviated rev; the receipt records the full hash.
    fleet
        .install(&name(), &from, &GitRev::parse(&rev1[..7]).expect("short"))
        .expect("install");
    assert_eq!(installed_rev(&case), rev1);
    assert_eq!(
        std::fs::read_to_string(case.plugins_dir().join("palette/plugin.lua")).expect("tree"),
        "version = 1\n",
        "checkout materializes the pinned revision"
    );

    // A second commit after install exercises the fetch path on update.
    let rev2 = commit_more(&repo, "version = 2\n", "rev2");
    fleet
        .update(&name(), &GitRev::parse(&rev2).expect("rev2"))
        .expect("update");
    assert_eq!(installed_rev(&case), rev2);
    assert_eq!(
        std::fs::read_to_string(case.plugins_dir().join("palette/plugin.lua")).expect("tree"),
        "version = 2\n"
    );

    // Rollback restores rev1 and consumes the undo level.
    fleet.rollback(&name()).expect("rollback");
    assert_eq!(installed_rev(&case), rev1);
    assert_eq!(
        std::fs::read_to_string(case.plugins_dir().join("palette/plugin.lua")).expect("tree"),
        "version = 1\n"
    );
    let second = fleet.rollback(&name());
    assert!(second.is_err(), "second rollback fails without an update");
}

#[test]
fn repository_hooks_never_execute() {
    let case = Case::new("hooks");
    let (repo, rev1) = fixture(&case, "origin", "version = 1\n");
    let fleet = case.fleet();
    fleet
        .install(
            &name(),
            &source(&repo),
            &GitRev::parse(&rev1).expect("rev1"),
        )
        .expect("install");

    // Plant an executable post-checkout hook in the INSTALLED clone: any
    // checkout that honors hooks would create the marker.
    let marker = case.root.join("hook-marker");
    let hook = case.plugins_dir().join("palette/.git/hooks/post-checkout");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ntouch \"{}\"\n", marker.display()),
    )
    .expect("plant hook");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook).expect("hook").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook, perms).expect("chmod");
    }

    let rev2 = commit_more(&repo, "version = 2\n", "rev2");
    fleet
        .update(&name(), &GitRev::parse(&rev2).expect("rev2"))
        .expect("update");
    assert!(!marker.exists(), "update must not execute repository hooks");
    fleet.rollback(&name()).expect("rollback");
    assert!(
        !marker.exists(),
        "rollback must not execute repository hooks"
    );
}

#[test]
fn hostile_cli_inputs_fail_with_zero_side_effects() {
    let case = Case::new("hostile-cli");
    let binary = env!("CARGO_BIN_EXE_bitty-plugin-manager");
    let plugins = case.plugins_dir();
    let hostile: Vec<Vec<&str>> = vec![
        vec![
            "install",
            "../escape",
            "--from",
            "/tmp/x",
            "--rev",
            "3a26417",
        ],
        vec!["install", "UPPER", "--from", "/tmp/x", "--rev", "3a26417"],
        vec![
            "install",
            "palette",
            "--from",
            "ext::sh -c whoami",
            "--rev",
            "3a26417",
        ],
        vec![
            "install",
            "palette",
            "--from",
            "/tmp/x",
            "--rev",
            "--upload-pack=evil",
        ],
        vec!["install", "palette", "--from", "/tmp/x", "--rev", "HEAD"],
        vec!["update", "palette", "--rev", "main"],
        vec!["rollback", "a/b"],
        vec!["frobnicate", "palette"],
    ];
    for argv in hostile {
        let output = Command::new(binary)
            .arg("--plugins-dir")
            .arg(&plugins)
            .args(&argv)
            .output()
            .expect("binary runs");
        assert!(!output.status.success(), "{argv:?} must exit non-zero",);
        assert!(
            !String::from_utf8_lossy(&output.stderr).is_empty(),
            "{argv:?} must explain itself on stderr"
        );
    }
    let entries: Vec<_> = std::fs::read_dir(&plugins)
        .map(|entries| entries.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        entries.is_empty(),
        "hostile inputs leave zero side effects, found {entries:?}"
    );
}

#[test]
fn foreign_dirs_and_corrupt_receipts_fail_closed() {
    let case = Case::new("receipts");
    let fleet = case.fleet();

    // Absent entry: update/rollback/status report, they do not invent.
    assert!(fleet.installed(&name()).expect("read").is_none());
    assert!(
        fleet
            .update(&name(), &GitRev::parse("3a26417").expect("rev"))
            .is_err()
    );
    assert!(fleet.rollback(&name()).is_err());

    // Foreign directory without a receipt is never adopted.
    std::fs::create_dir_all(case.plugins_dir().join("palette")).expect("foreign dir");
    std::fs::write(case.plugins_dir().join("palette/plugin.lua"), "hi\n").expect("file");
    assert!(
        fleet.installed(&name()).is_err(),
        "foreign dir fails closed"
    );

    // Corrupt receipt fails closed.
    std::fs::write(
        case.plugins_dir().join("palette/.bitty-manager"),
        "repo=https://example.com/o/r\nrev=zzz\nprev=none\n",
    )
    .expect("corrupt receipt");
    assert!(
        fleet.installed(&name()).is_err(),
        "corrupt receipt fails closed"
    );
}

#[test]
fn cli_status_reports_lifecycle() {
    let case = Case::new("cli-status");
    let (repo, rev1) = fixture(&case, "origin", "version = 1\n");
    let binary = env!("CARGO_BIN_EXE_bitty-plugin-manager");
    let plugins = case.plugins_dir();

    let run = |argv: &[&str]| {
        let output = Command::new(binary)
            .arg("--plugins-dir")
            .arg(&plugins)
            .args(argv)
            .output()
            .expect("binary runs");
        assert!(
            output.status.success(),
            "{argv:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf8")
    };

    let before = run(&["status", "palette"]);
    assert!(
        before.contains("not installed"),
        "status before install: {before}"
    );
    run(&["status", "palette"]);
    let short = rev1[..7].to_string();
    let installed = run(&[
        "install",
        "palette",
        "--from",
        repo.to_str().expect("path"),
        "--rev",
        &short,
    ]);
    assert!(installed.contains("installed palette"), "{installed}");
    let after = run(&["status", "palette"]);
    assert!(after.contains(&rev1), "status shows the full hash: {after}");
    assert!(
        after.contains("prev=none"),
        "fresh install has no rollback: {after}"
    );
}
