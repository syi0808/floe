# Ordinary startup and dashboard correction

The user rejected the profile-selection concept and requested ordinary startup. The earlier cutover had introduced an existing-profile-only screen without a fresh-install path; this was not a requested product concept.

## Internal installation

Rust now owns one internal installation through open_default(ApplicationSupport). Flutter receives admitted Person/device identity from its host, removes the chooser and independent identity-file parsing, and enters the existing app/Vault flow. Existing valid identity is reused; fresh installation persists random stable identity before create-only plain-store initialization. Explicit database-open remains for diagnostics. The installation lease remains in the shared store until every owner retires.

The user's development policy permits recoverable local reset for classified corrupt/unsupported/inconsistent state, without migration. Debug builds move exact Floe data/identity entries into a same-filesystem recovery directory with durable manifest/continuation intent. Diagnostics, journals, historical recovery evidence and old Keychain slots are preserved; external provider data is unaffected. Existing encrypted host locks are held through moves. Older app binaries do not participate in the new installation lease and must be closed before development recovery. Release builds never automatically archive/reset.

A read-only encrypted Vault preflight distinguishes missing/malformed keys, successful metadata mismatch and typed Corrupt/NotAdb from access denial, busy locks, I/O and generic database errors. Generic encrypted errors cannot prove wrong key and remain unavailable. The inherited synchronous Keychain read has no hard elapsed-time deadline. No normal Vault initializer or journal replay runs inside preflight. Fresh plain-store initialization checkpoints and syncs before ready publication; this is not cleanup of an existing user's journals.

The existing Conversation Vault create/unlock flow remains. A successful zero-session query returns an explicit conversation_session_absent result; the client then invokes the existing idempotent Start Session command. Other read failures remain errors. A separately discovered normal Conversation schema validator ordering bug is corrected so valid fresh data reopens without reset.

## Dashboard

Login acceptance and dashboard-state readiness are distinct. State failures preserve the authenticated retry shell and hide stale controls; retries use GET, without minting another session. Only actual401 returns token entry. Typed Trust unavailability remains503 through both session admission and trace-state reads. Authentication, CSRF, token bytes and rotation policy are unchanged.

The reported unauthorized message is not yet attributed: the previous UI conflated login401 and subsequent state401. No token was retrieved, used, logged or transmitted by this source correction.

## Verification boundary

This source batch is newer than e02779bf's scoped G2 pass. It requires coherent Rust/Go compilation, Dart analysis and affected Apple builds before manual use. No automated tests, model/provider calls, real-user bootstrap/reset/key operations or log deletion have been executed. Preserve existing user app processes and evidence until a coordinated manual restart.


## Compiler boundary on cbf1c7d0

Full cloud Rust build passed (50.18 seconds), dependency policy passed (23 nodes/126 edges), and Dart analysis passed with zero errors, zero warnings and 128 informational diagnostics. Default VCS-stamped Go executable build passed from an independent clone, recording exact cbf1c7d0 and vcs.modified=false; the earlier linked-worktree VCS failure was environment-related. A single authorized cargo fmt pass produced only 12 Rust source formatting changes, no lock or Go changes; that captured patch is integrated. No rebuild or tests were run solely for formatting. Full Apple application builds await the formatted checkpoint. These are compiler results, not startup/reset or dashboard-authentication behavior verification.
