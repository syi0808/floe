# Active execution plan

Use [the architecture completion plan](2026-10-02-architecture-refactor.md) for the remaining P0–P6 sequence and its current checkpoint. It is the only active migration/execution plan. Read current source and [architecture](../architecture/README.md) for implementation reality.

The current Voice checkpoint composes the process-scoped media registry and
Gateway live provider into App generations, drains exact leases through durable
Stop receipts, and exposes durable Start/status through the typed FFI and Flutter
screen. The active plan records current verification and remaining gates. This
cloud build has no native audio backend, so App Start is durably cancelled
before model/media preparation; it does not claim microphone capture or a
provider call. Apple WebRTC/audio, OS-backed prior-process evidence and Apple device
qualification remain open. Historical P1 or T0/S1/G1/S2/G2/S3 labels are not
instructions to restart completed work.

Durable reasons stay in [ADRs](../decisions/README.md). Shared boundary contracts belong in [architecture](../architecture/README.md). [Legacy behavior evidence](../testing/legacy-behavior/README.md) remains available for test reconstruction. Retired cutover sheets and reports are in [Git history](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/plans/).

Update the active plan when order or scope changes. Do not append another global status document or turn static review counts into runtime qualification.
