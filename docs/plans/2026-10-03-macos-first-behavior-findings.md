# First macOS behavior investigation

The user reported a failed response to “오늘 일정 알려줘” while Day displayed Calendar records. Incident reference: 57ae5abb-abc4-44bc-83d4-263da053872c. The original app, logs, journals and debug records are preserved. Local read-only diagnosis continues; the exact incident cause is not yet established.

## Observed binary provenance

Read-only Mac diagnosis found two clients: the verified 8cb build was at profile selection, while the separate user-workspace app had unknown binary revision. The original 14:57:22 UTC generic Stalled/session record is confirmed, but it must not be attributed to the verified 8cb artifact. Database read access was locked; no bypass or destructive recovery is authorized.

## Independently confirmed source corrections

- EventKit setup and Actions use canonical execution owner `apple:<admitted device_id>`, but assistant read/review/metadata/dependency paths compared bare device IDs. Eleven reader/plumbing files now use the canonical owner for EventKit only. The grant lookup accepts an exact typed ExecutionOwnerId; native OS requests and reauthorization receive the admitted bare device separately. No aliases, source/grant migration, Attention expansion or data reset. Day product display and assistant read authority remain separate.
- The Device adapter imposed an unexplained fixed 4096-token minimum despite the admitted 1024-token finalization partition. Remove that minimum, retain positive admitted allowance and all profile byte/capability/deadline limits, and cap response tokens at min(profile limit, admitted allowance). Ledger accounting and unknown-usage treatment are unchanged.
- Finalization discarded any Engine error and later reported Stalled. It now propagates the typed failure; a completed run lacking output alone uses AttemptedWithoutReply. Original exhaustion remains in the journal.
- Every generic Stalled failure displayed a Calendar-specific repeat claim. All English localization sources now say Floe could not complete a response within the run. This does not claim a particular tool repeated.

These are source-level defects found during incident triage, not proof that all caused this occurrence. No executable behavior test, model/provider call or data/credential mutation was performed by this correction. The full correction batch requires compilation and a coordinated rebuilt app before retesting. Original running app and records must remain available until diagnosis is complete.
