# Migration boundary: Core distribution capability vs manager #10/#11 (issue #18)

Docs-only boundary record for manager issue #18 (audit finding F8 in
CTX-1085 `research/review/2026-10-10/core-split-residual-audit.md`, under
umbrella bitty-terminal/bitty#1629). This change extends the #10/#11
acceptance language to cover absorbing the Core-side distribution surface,
and authorizes no code.

- Prerequisite is W-72 (bitty-docs CTX-0261 Issue #401, closed, boundary
  defined) with umbrella bitty-terminal/bitty#1629 still open (W-129).
- Deferred until after the 0.0.23 release; issue #18 is the tracking entry
  so the debt is not lost.
- This record does not revisit the scopes owned by the neighboring tasks:
  contract readiness (`docs/contract-readiness.md`), partial implementation
  state (`docs/implementation-status.md`), and the deferred verification
  plan (`docs/verification-plan.md`).

## Moves here (bitty-plugin-manager, via #10/#11)

The distribution capability Core still carries moves to this repository:

- Trust: publisher trust modes V-A/V-B/V-C and Ed25519 verification
  (today in `crates/bitty-package/`, self-labelled "draft / not
  normative").
- Resolver and version solving: dependency version solving and
  version-constraint solving across both fleets, plus the registry index
  reads that feed solving (today split between `crates/bitty-package/`
  `resolver` and `crates/bitty-plugin-host/src/registry.rs` `resolve_all`
  with interface version solving).
- Lockfile: the lockfile digest triple (today in `crates/bitty-package/`
  `lockfile`).
- Catalog and registry grammar: the catalog surface and the registry URL
  grammar (today in `crates/bitty-package/` `catalog` and `source`).
- Install verification: the 7-stage install-time verification (fetch
  framing, checksums, capability diff, TOFU pinning, signature verify;
  today in `crates/bitty-plugin-host/src/install.rs`).
- Seed installability stays where #11 put it: the manager only needs to be
  fetchable and verifiable through the seed (see the overlap note below).

## Stays in plugin-host as mechanism

- The in-memory lifecycle registry: `resolve_all` as a runtime mechanism
  stays in `crates/bitty-plugin-host/src/registry.rs`; only the solving
  half is this repository's registry-index job (#11).
- Core startup still independently checks installed integrity,
  compatibility, and grants; installation executes no package code. The
  transactional activation, rollback, checksum/signature policy, and grant
  review invariants are unchanged by this move.

## Stays in the terminal CLI as a thin client

- `bitty plugin install|remove|enable|disable|revoke|info` (today in
  `crates/bitty-terminal/src/plugin.rs`) becomes a thin client over the
  trait/CLI handoff to this manager. Git sources stay disabled there until
  the manager side lands.

## Overlap note: the shipped component-install seed is not duplicated here

- bitty#1791 (minimal component-install seed, option A, shipped with the
  bitty binary) and bitty#1792 (R2 prebuilt component distribution) both
  shipped in the 0.0.23 track: R2 fetch via fixed-argv system curl plus
  SHA256 verify, with the URL allowlist pinned to the upstream host.
- Manager issue #10 (upstream R2 prebuilt fleet) is closed against that
  track. The seed bootstraps this manager — the manager itself is
  installable via the seed on a fresh machine — and nothing in that seed
  is re-implemented here.

## Extended #10/#11 acceptance

- #10 acceptance is met and the issue is closed (upstream R2 fleet against
  the bitty#1792 artifacts).
- #11 acceptance (registry-driven install/update/rollback for both fleets
  on a cargo-less machine; hostile inputs refused with zero spawn; manager
  itself installable via seed on a fresh machine) is extended by this
  boundary: landing #11 additionally means the Core-side surface above has
  a manager-side home, with the trait/CLI handoff keeping `bitty plugin`
  a thin client.
- The Core-side removal follow-up is referenced from bitty-terminal/bitty#1629
  and proceeds once #10/#11 have landed. Verification of this record is
  metadata only through the fast docs gates.
