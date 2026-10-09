//! bitty-plugin-manager: candidate external package manager for Bitty.
//!
//! Slice 1 (issue #9) implements the **downstream Lua-plugin fleet** only:
//! each plugin lives as a validated git checkout under a plugins directory,
//! and the manager provides install, update, and single-level rollback.
//! The upstream R2 prebuilt fleet, the registry index with version solving,
//! and seed bootstrap are deferred to tracked follow-ups (see README).
//!
//! # Safety contract
//!
//! - Validation first: [`PluginName`], [`GitRev`], and [`PluginSource`] are
//!   the only way untrusted strings enter the system. Constructors reject
//!   hostile inputs (path traversal, option injection, non-https remotes)
//!   and return [`Error`] **before any process is spawned**.
//! - No shell: git runs with a fixed argument vector via [`SystemGit`];
//!   package checkouts are never executed (all git invocations disable hooks
//!   with `-c core.hooksPath=/dev/null`).
//! - Fail-soft: every failure leaves the previous installation intact or
//!   reports exactly what to clean up; [`Fleet::rollback`] restores the
//!   previously installed revision.

mod error;
mod fleet;
mod git;
mod name;
mod receipt;
mod rev;
mod source;

pub use error::{Error, ErrorKind};
pub use fleet::{Fleet, Installed};
pub use git::{GitOutput, GitRunner, SystemGit};
pub use name::PluginName;
pub use receipt::Receipt;
pub use rev::GitRev;
pub use source::{PluginSource, SourceKind};
