# G1 first compiler pass

Input: published source `3a17f45abe1c73d87a67933fd8d6df023a84e033`, tree `ceee00c7731dce9c75221be8c2ae59300573ab10`, production-equivalent to local `69e6c427aab0b3dbcbe194281536d05abf1bd6cf`. Later documentation publication does not change these inputs.

The Mac completed the first pass on 2026-10-03. Cargo formatting, changed Go formatting, and changed Dart formatting passed. Returned formatting changes and lockfile resolution changes are integrated into the correction batch. Cargo lock changes describe workspace dependency edges without version upgrades; Flutter resolution removed eleven obsolete test dependencies without upgrades.

- Go: `GOTOOLCHAIN=local go build ./internal/...` passed.
- Apple: all fifteen planned compiler-only phases passed, including the actual runner imports. Six initially sandbox-blocked phases passed after normal reviewed access. These are not application builds or runtime tests.
- Rust: workspace library/binary check failed on missing Apple Keychain bindings and a missing category Hash implementation. The example check additionally exposed an obsolete TaskSnapshot field and an ambiguous integer counter. Downstream compilation may expose further errors after these blockers are corrected.
- Dart: static analysis returned one error, seven warnings, and 107 informational lints. The error is a missing required field bound in the attention acquisition parser. Informational style diagnostics are not 107 compilation failures.

G1 remains open. Corrections must retain fail-closed keychain behavior, exact Task identity, bounded iteration accounting, and source authority. Rerun affected Rust checks with `--keep-going` and Dart analysis after one coherent correction batch. Passed unchanged Go and Apple phases need not be repeated. No tests, application packaging, live credential/provider operations, or user-data reset occurred. S2 production edits remain gated on G1 closure.

Evidence archive SHA-256: `658adbacb504a21602d9aadef9d9dd87280aac1abbd1aceb14a027480ed5c4e7`. Full command, elapsed-time, output, and platform evidence are retained in the G1 log artifact.

## First correction batch

- Bind the actual missing Security.framework constants rather than substituting authentication-UI Skip. Enable the existing security-framework-sys OSX_10_15 feature and select macOS Data Protection Keychain so nonsynchronizing ThisDeviceOnly accessibility is enforced. Existing durable pairing expectations preserve RepairRequired when a historical slot is absent; no fallback or live credential operation is introduced.
- Derive Hash for the closed review category DTO used by duplicate rejection.
- Use TaskSnapshot.task_id.as_uuid() for transcript coverage identity and explicitly type the bounded journal iteration counter as u32.
- Remove compiler-reported unused imports and the uncalled legacy source classifier. Transcript projection remains conservative; its unused receipt argument is removed together with all three Vault callers, whose admission/receipt validation remains in place.
- Correct the attention parser's required Person UUID bound and the seven Dart warnings. Informational style lints are deferred; they are not compilation errors.

These changes are source corrections pending the next coordinated G1 compiler pass, not evidence of a passing Rust or Dart gate.

## Second compiler pass and correction batch

Input `3a698d76b66cec22a4cc59572afa3c90c3a1bf51`. The affected Rust checks ran with keep-going; Dart analysis exited zero with no errors or warnings and the 107 unchanged informational lints. Rust reached the provider and Vault adapters and reported fourteen diagnostics, including cascaded type-inference errors. No lockfile changes occurred. Nine Rust formatter changes are integrated; Dart formatting changed no files, but its launcher exit reflected telemetry-home write permission rather than a source-format diagnostic.

Corrections preserve typed Task IDs and canonical Task/Artifact evidence; source operation storage uses the ConnectionId accessor. Calendar action validation receives the actual Connections repository at construction. Removed obsolete per-session attempt-list pruning without changing immutable Run journals or aggregate usage. Knowledge records Blocked with exact Run/review IDs; its Completed-only evidence and learner admission remain unchanged. Provider signed-consumer verification must preserve the admitted consumer kind and exact signed identifier without guessing builtin authority.

Rust compilation remains pending after this correction batch. Passed unchanged Dart/Go/Swift phases are retained, not rerun without new affected changes. Protocol unused validator warnings remain visible; no validation contract was removed solely to silence them.
