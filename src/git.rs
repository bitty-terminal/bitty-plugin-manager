//! Fixed-argument git execution. [`GitRunner`] is the seam between the
//! fleet logic and process spawning: production uses [`SystemGit`], tests
//! use an in-memory fake (see the `tests` module below). There is no shell
//! anywhere on this path — arguments are passed as an argv vector — and
//! every invocation disables repository hooks (`-c core.hooksPath=/dev/null`)
//! plus terminal prompts, so installing a plugin can never execute package
//! code through git.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use crate::{Error, ErrorKind};

/// Maximum stderr bytes kept in a [`Error`] for a failed git invocation.
pub const MAX_STDERR_BYTES: usize = 2048;

/// Captured result of one git invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    /// Standard output, trimmed of trailing newlines.
    pub stdout: String,
    /// Standard error (kept for diagnostics only).
    pub stderr: String,
}

/// Runs git with a fixed argv in a working directory.
pub trait GitRunner {
    /// Run `git <argv>` in `cwd`. Returns [`Error`] of kind
    /// [`ErrorKind::Git`] when git cannot start, exits non-zero, or emits
    /// non-UTF-8 output.
    fn run(&self, argv: &[&str], cwd: &Path) -> Result<GitOutput, Error>;
}

/// Production runner: spawns the system `git` binary.
#[derive(Debug, Clone)]
pub struct SystemGit {
    program: OsString,
}

impl SystemGit {
    /// Use `git` from `PATH`.
    pub fn new() -> Self {
        Self {
            program: OsString::from("git"),
        }
    }
}

impl Default for SystemGit {
    fn default() -> Self {
        Self::new()
    }
}

impl GitRunner for SystemGit {
    fn run(&self, argv: &[&str], cwd: &Path) -> Result<GitOutput, Error> {
        let mut cmd = Command::new(&self.program);
        for var in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_COMMON_DIR",
            "GIT_NAMESPACE",
            "GIT_CEILING_DIRECTORIES",
        ] {
            cmd.env_remove(var);
        }
        let output = cmd
            .arg("-c")
            .arg("core.hooksPath=/dev/null")
            .args(argv)
            .current_dir(cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|err| Error::new(ErrorKind::Git, format!("cannot start git: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut clipped: String = stderr
                .trim()
                .chars()
                .map(|c| {
                    if c.is_control() && c != '\n' && c != '\t' {
                        '?'
                    } else {
                        c
                    }
                })
                .collect();
            if clipped.len() > MAX_STDERR_BYTES {
                let mut cut = MAX_STDERR_BYTES;
                while !clipped.is_char_boundary(cut) {
                    cut -= 1;
                }
                clipped.truncate(cut);
            }
            return Err(Error::new(
                ErrorKind::Git,
                format!("git {} failed: {clipped}", argv.first().unwrap_or(&"?")),
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| Error::new(ErrorKind::Git, "git emitted non-UTF-8 output".to_string()))?;
        Ok(GitOutput {
            stdout: stdout.trim_end().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! In-memory [`GitRunner`](super::GitRunner) for zero-spawn tests:
    //! records every invocation and answers scripted outputs, so tests can
    //! prove hostile inputs never reach a spawn.

    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use super::{GitOutput, GitRunner};
    use crate::Error;

    /// Scripted runner. `push_output` queues answers; `spawns()` reports how
    /// many invocations happened.
    pub struct FakeGit {
        calls: Mutex<Vec<(Vec<String>, PathBuf)>>,
        outputs: Mutex<VecDeque<Result<GitOutput, Error>>>,
    }

    impl FakeGit {
        /// New fake with no scripted outputs (every call fails closed).
        pub fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                outputs: Mutex::new(VecDeque::new()),
            }
        }

        /// Queue one scripted answer for the next invocation.
        pub fn push_output(&self, output: Result<GitOutput, Error>) {
            self.outputs.lock().expect("fake lock").push_back(output);
        }

        /// Script a successful invocation emitting `stdout`.
        pub fn push_stdout(&self, stdout: &str) {
            self.push_output(Ok(GitOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
            }));
        }

        /// Number of invocations so far.
        pub fn spawns(&self) -> usize {
            self.calls.lock().expect("fake lock").len()
        }

        /// Argv of every invocation, in order.
        pub fn argv_log(&self) -> Vec<Vec<String>> {
            self.calls
                .lock()
                .expect("fake lock")
                .iter()
                .map(|(argv, _)| argv.clone())
                .collect()
        }
    }

    impl Default for FakeGit {
        fn default() -> Self {
            Self::new()
        }
    }

    impl GitRunner for FakeGit {
        fn run(&self, argv: &[&str], cwd: &Path) -> Result<GitOutput, Error> {
            self.calls.lock().expect("fake lock").push((
                argv.iter().map(|s| s.to_string()).collect(),
                cwd.to_path_buf(),
            ));
            self.outputs
                .lock()
                .expect("fake lock")
                .pop_front()
                .unwrap_or_else(|| {
                    Err(Error::new(
                        crate::ErrorKind::Git,
                        "fake git: no scripted output".to_string(),
                    ))
                })
        }
    }

    #[test]
    fn fake_records_and_replays() {
        let fake = FakeGit::new();
        fake.push_stdout("abc123");
        let out = fake
            .run(&["rev-parse", "HEAD"], Path::new("/tmp"))
            .expect("scripted");
        assert_eq!(out.stdout, "abc123");
        assert_eq!(fake.spawns(), 1);
        assert_eq!(
            fake.argv_log(),
            vec![vec!["rev-parse".to_string(), "HEAD".to_string()]]
        );
    }
}
