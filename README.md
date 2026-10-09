# bitty-plugin-manager

Candidate external package manager for Bitty plugins (issue #9).
Read [AGENTS](AGENTS.md). Task management lives in CarryCtx.

## Status

Slice 1 implemented: the **downstream Lua-plugin fleet** (git checkouts).
Each plugin lives as a full git checkout under the plugins directory
(`$XDG_DATA_HOME/bitty/plugins/<name>/`, overridable with `--plugins-dir`),
pinned to a registry revision. Update fetches and checks out the new
revision; rollback restores the previous one. The crate has zero
dependencies (standard library plus the system `git` binary).

Deferred to ordered follow-ups (upstream R2 prebuilt components depend on
bitty#1792 artifacts; seed bootstrap on bitty#1791; registry index with
version solving is manager-side design):

- Upstream R2 prebuilt fleet (fixed-argv curl + tar + SHA256 verify):
  issue #10.
- Registry index, version-constraint solving, and fail-soft policy, plus
  seed installability: issue #11.

## Usage

```sh
cargo build --release  # produces target/release/bitty-plugin-manager
bitty-plugin-manager install palette --from https://github.com/bitty-terminal/palette --rev 3a26417
bitty-plugin-manager update palette --rev ff00000
bitty-plugin-manager rollback palette
bitty-plugin-manager status palette
```

Sources are `https://host/path` remotes or absolute local paths (the
latter serve air-gapped installs and offline tests). Revisions are full or
abbreviated commit hashes, never branch names.

## Safety contract

- Validation first: names (`^[a-z0-9]([a-z0-9-]*[a-z0-9])?$`, at most 64
  bytes), revisions (7-64 hex characters), and sources (bare-host https or
  absolute `..`-free paths) are checked before any process spawns. Hostile
  inputs are refused with zero spawn.
- No shell, no package code: git runs with a fixed argument vector, every
  invocation disables hooks (`-c core.hooksPath=/dev/null`) and terminal
  prompts, and nothing inside a checkout is ever executed.
- Fail-soft: install never overwrites (staged clone, verified HEAD, atomic
  rename); a failed update keeps the old checkout and receipt; rollback
  restores the previous revision and consumes the undo level.
- The host still independently checks installed integrity, compatibility,
  and grants at startup; installation executes no package code.

## Development

```sh
just check  # prettier + markdownlint + metadata + hygiene + paths + cargo fmt/clippy/test
```

`cargo test` runs unit tests (validation corpora, scripted-runner
zero-spawn proofs, receipt round-trips) plus the real-git integration
suite (`tests/downstream_fleet.rs`: lifecycle, hook non-execution,
hostile CLI inputs, corrupt receipts). MSRV is 1.85 (`cargo +1.85 test`);
the pinned channel lives in `rust-toolchain.toml`.

Prerequisite: W-72 / bitty-docs CTX-0261, Issue #401 (closed, boundary
defined) and bitty#1629 umbrella (open, W-129). CTX-0001 (bootstrap) is
complete; this slice implements issue #9 under CTX-0003 (issue #2), with
CTX-0002 (issue #3, contract readiness) still pending acceptance.
