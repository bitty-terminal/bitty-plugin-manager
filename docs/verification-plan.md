# Verification plan (deferred)

Independent verification for issue #1 is blocked. No verification evidence exists and none is claimed.

- Verification requires a different reviewer from implementation, real platform and negative-path evidence (hostile inputs with zero spawn, hook non-execution, corrupt-receipt fail-closed, rollback recovery), canonical documentation synchronization, and no weakening of Core invariants (startup still independently checks integrity, compatibility, and grants; installation executes no package code).
- This phase is blocked on CTX-0002 contract acceptance (issue #3, PR #13 deferred) and CTX-0003 implementation landing (issue #2: downstream in open PR #12, upstream deferred to #10, registry/seed to #11; partial-state recorded in PR #14). Umbrella bitty-terminal/bitty#1629 remains open.
- Product verification remains absent on `main` (no `src/`). Just check runs formatting, Markdown lint, metadata, hygiene, and path checks and proves no package behavior.
