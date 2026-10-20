# Active execution plan

Use [the architecture completion plan](2026-10-02-architecture-refactor.md) for the remaining P0–P6 sequence and its current checkpoint. It is the only active migration/execution plan. Read current source and [architecture](../architecture/README.md) for implementation reality.

The current Voice checkpoint follows the retained Start preparation controller.
Durable Surface/Work admission, existing-Manager dispatch, result publication,
pending-work cancellation, and Start command custody/preparation are qualified.
The native crate now contains a process-scoped exact-media lease registry with
synthetic Stop-observation tests. The cloud Rust workspace gate passes 573
tests with one explicit Go/Rust case ignored by default; that synthetic fixture
also passed when invoked separately. App/provider composition, the Apple audio
backend, Flutter/FFI Start/status controls and Apple device qualification remain
open. Consult the final Voice checkpoints in the active plan for the exact
evidence and sequence. Historical P1 or T0/S1/G1/S2/G2/S3 labels are not
instructions to restart completed work.

Durable reasons stay in [ADRs](../decisions/README.md). Shared boundary contracts belong in [architecture](../architecture/README.md). [Legacy behavior evidence](../testing/legacy-behavior/README.md) remains available for test reconstruction. Retired cutover sheets and reports are in [Git history](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/plans/).

Update the active plan when order or scope changes. Do not append another global status document or turn static review counts into runtime qualification.
