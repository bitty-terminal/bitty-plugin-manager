# Implementation status (partial)

Phase 3 implementation for issue #2 is partially present on a feature branch; nothing is merged to `main`.

- Implemented (unmerged): downstream Lua-plugin fleet as validated git checkouts (install/update/rollback/status, zero dependencies, fixed-argv system git, hooks disabled, receipts with single-level rollback) on branch `issue-9/downstream-fleet` (open PR #12, Refs #9, 2121 insertions). Gates on that branch: `just check` green (21 unit + 5 real-git integration pass), MSRV 1.85 build/clippy/test green, actionlint/act/gitleaks clean. Negative supply-chain evidence there: hostile name/rev/source corpora refused with zero spawn, planted hook never executes, corrupt receipts fail closed.
- Deferred (ordered): upstream R2 prebuilt fleet (issue #10, blocked on bitty#1792 artifacts) and registry index with version-constraint solving plus seed installability (issue #11, blocked on bitty#1791 seed).
- Ordering note: this work proceeds under CTX-0003 (issue #2) with CTX-0002 (issue #3, contract readiness) still pending acceptance — flagged, not bypassed. Phase 3 cannot close until CTX-0002 is accepted and the deferred slices land.
- This change authorizes no code on `main`; it only records the partial state. Verification is metadata only through just check on `main`.
