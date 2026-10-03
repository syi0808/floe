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


## Apple build closure on 6dabf448

macOS, arm64 iOS simulator and unsigned device application builds passed on the formatted source. Artifact architecture and four native exports were verified; macOS/simulator signature checks passed. Source and lockfiles remained clean. Full evidence is retained in Library file libfile_361aecfee5848191994efbbb914eaa83. Together with the preceding Rust/Go/DAG/Dart results, this closes build validation for this correction. Startup/development recovery and the reported dashboard authentication sequence still require coordinated behavior observation. No validator launched the apps or reset user state.


## Fresh plain-store schema reproduction

The user reported unsupported plain schema during startup. An isolated pinned-Turso reproduction on the validated source showed both fresh create_new and reopen failing: Turso renders stored CREATE SQL with punctuation spacing such as CHECK (id = 1), while the validator expected CHECK(id = 1). Seventeen of 26 objects differed by formatting. Authored schemas match; this is not evidence that user data is corrupt, and further user resets are unnecessary.

A reviewed narrow correction compares bounded SQL token sequences. It ignores inter-token whitespace and unquoted ASCII keyword/identifier case, while preserving quoted content, numeric spelling, compound operators, constraints and order. IF NOT EXISTS is normalized only at the CREATE prefix. Stored invalid/different definitions remain rejected; malformed expected definitions are internal Storage failures. Safe mismatch metadata is available at StoreError but still flattened by the existing App startup string projection.

The corrected patch is published as unverified because artifact transfer to the isolated worker failed. Next validation must fetch this exact commit and prove fresh create/close/reopen succeeds, all 26 schema objects match, and a scratch replacement CHECK(id >= 1) with unchanged row(1,1) is rejected. Additional lexical negatives preserve literal case, word boundaries, compound operators and key constraints. This is isolated behavior reproduction, not automated suite reconstruction. No user database, key or recovery archive was modified.


## Focused schema correction validation on c3c10528

The exact published patch passed isolated fresh create/drop/reopen, reopening a copy of the original failing scratch database, and all 26 schema definitions. A changed CHECK(id >= 1) with unchanged marker row(1,1) was rejected with definition_mismatch metadata. Five lexical cases passed: formatting equivalence plus preservation of quoted literal case, word boundaries, operator meaning and UNIQUE. Scratch evidence remains preserved; no user database was opened or reset and no persistent test suite was rewritten.

The isolated worker used Rust1.99.0 with the pinned dependency lock; this establishes the reproduced behavior but does not substitute for the supported Rust1.93.1 Apple build. The exact two-file formatting-only delta (14 additions/13 deletions) is integrated. Next gate is the rebuilt macOS artifact, followed by affected scoped Apple coverage and coordinated user retest.


## Encrypted Vault creation and Connections readiness

The user confirmed ordinary app startup after the plain-schema fix. Subsequent Remote server preparation displayed a generic owner error. Existing diagnostic records showed the preceding local Vault create failed with unsupported_version; no Gateway HTTP request is part of Prepare Setup. A scratch encrypted Vault using only an in-memory fake key provider reproduced the same create/open failure: Knowledge's first schema validator still used whitespace-only text comparison.

All four encrypted DDL comparator families (Knowledge, Actions, Gateway receipt storage and Expert binding) now share the proven token comparator. Existing object-set, version, trigger/index and semantic checks remain. SQL read failures retain storage errors rather than pretending unsupported schema. The Expert binding comparator no longer strips whitespace inside literals. No migration or real Keychain action is included. This source extension awaits exact-checkpoint fake-key create/reopen verification.

Settings previously started the existing Vault/Conversation load asynchronously while Connections rendered active controls and discarded typed errors. The client now observes that same lifecycle, disables Connections commands until actual Vault Ready, refreshes after readiness and fences late presentation completions. It creates no separate Vault workflow and does not require a successful Conversation Session once Vault is ready. Correlated diagnostic records contain only safe typed error fields/request IDs and sanitized summaries; displayed errors retain their Error ID. Existing uncertain command IDs survive readiness changes. The pre-existing terminal-versus-uncertain classification of individual native errors is unchanged.

Dashboard login was separately confirmed by the user after deleting stale browser cookies. The proposed compatibility-cookie source change was cancelled and reverted before any commit. No cookie cleanup code or agent credential action was published.


## Encrypted correction proof on 2e7d2983

Scratch encrypted Vault create/drop/reopen passed with the in-memory fake key provider. A copied scratch profile with Learning CHECK changed from version=1 to version>=0, retaining marker(1,1), was rejected as UnsupportedVersion. All four consumers use the shared comparator; no duplicate text normalizers remain. Rust formatting produced no delta. No OS Keychain/user-data/pairing operations were involved.

Dart analysis passed with zero errors, zero warnings and 132 informational diagnostics. One formatting pass changed five of eight targeted files; the formatter completed writes then returned1 on denied analytics-session metadata access, with no retry. The exact captured formatting-only patch is integrated; lockfile remained unchanged. Full app builds await this formatted checkpoint.
