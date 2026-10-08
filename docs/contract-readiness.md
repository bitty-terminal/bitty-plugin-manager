# Contract readiness (deferred)

Bitty-plugin-manager remains metadata-only on `main`. No boundary is accepted and no implementation is authorized until the umbrella and full public contract are accepted.

- Prerequisite is W-72 (bitty-docs CTX-0261 Issue #401, closed, boundary defined) with umbrella bitty-terminal/bitty#1629 still open (W-129).
- Local order is CTX-0001 complete, CTX-0002 blocked/in progress, CTX-0003 planned, CTX-0004 planned and blocked.
- Full contract readiness requires registry index with version-constraint solving and seed installability (tracked as issue #11, blocked on bitty#1791) plus upstream R2 prebuilt distribution (tracked as issue #10, blocked on bitty#1792). Downstream git-checkout fleet work proceeds under CTX-0003 (issue #2) via open PR #12 with CTX-0002 acceptance still pending — flagged, not bypassed.
- Acceptance for issue #3 requires an accepted and linked contract plus narrowed scope. That condition is not met, so this change only records readiness state and authorizes no code.
- Verification is metadata only through just check. Product gates remain absent on `main` (no `src/`); implementation evidence lives only on the `issue-9/downstream-fleet` branch (PR #12).
