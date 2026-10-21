# Active execution plan

Use [the architecture completion plan](2026-10-02-architecture-refactor.md) for the remaining P0–P6 sequence and its current checkpoint. It is the only active migration/execution plan. Read current source and [architecture](../architecture/README.md) for implementation reality.

The current Voice checkpoint composes the process-scoped media registry and
Gateway live provider into App generations, drains exact leases through durable
Stop receipts, and exposes durable Start/status and recovery across App restarts.
On an Apple host, only a fresh explicit Start passes the exact lease's microphone
permission gate, attaches its local audio track, and enables received audio before
the SDP offer is sent to the Gateway. Prepare/status/recovery stay muted. The
active plan records current verification and remaining gates; non-Apple/cloud
compositions remain fail-closed without an Apple media host. Synthetic tests and
builds do not qualify real capture, route changes, playback, or physical Stop.
Platform-backed evidence about an earlier OS process and Apple device
qualification remain open. Historical P1 or T0/S1/G1/S2/G2/S3 labels are not
instructions to restart completed work.

Durable reasons stay in [ADRs](../decisions/README.md). Shared boundary contracts belong in [architecture](../architecture/README.md). [Legacy behavior evidence](../testing/legacy-behavior/README.md) remains available for test reconstruction. Retired cutover sheets and reports are in [Git history](https://github.com/syi0808/floe/blob/45fe41af7c5d19a8c5675ecc342f1990ef6ebc00/docs/plans/).

Update the active plan when order or scope changes. Do not append another global status document or turn static review counts into runtime qualification.
