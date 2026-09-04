# S2 Contextual Focus Validation

Acceptance results belong in `PROGRESS.md`. S1 live verification remains a
prerequisite. [ADR 0009](../decisions/0009-contextual-focus-suggestion.md) records
the architecture and implementation-order exception.

## Automated validation

Run from the repository root:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p floe-ffi
(cd server && go test -race ./... && go vet ./...)
cd apps/client
flutter analyze
flutter test
FLOE_INFERENCE_TOKEN="$(openssl rand -hex 24)" flutter test integration/focus_inference_test.dart
flutter build macos --debug
```

The explicit cross-language test requires Go, a freshly built native library and
unused loopback port 8431. It builds/starts/stops the real Go service against a
synthetic HTTP model fixture and temporary config/database. Do not run it against
an existing personal gateway. It sends no external requests and uses no provider
credentials. Ordinary `flutter test` does not silently skip this separate suite.

Coverage:

- Rust: Person-scoped preference CRUD, reopen/CAS, deleted preference exclusion,
  minimal context, non-mutation, empty/past/fully occupied days, all-day blocks,
  midnight boundaries, malformed/oversized output, fabricated slots/sources,
  duplicate evidence, unknown action fields and preference changes during inference.
- Go: provider wire contracts, authentication, browser-origin denial, request/body
  bounds, external permission before any provider request, cloud-alias preflight,
  deadline/cancellation propagation, concurrency bound, errors/redaction and
  no retries/redirects. Inventory excludes endpoints and credential references.
- Flutter: actual JSON/C ABI preference lifecycle, stale/invalid writes, Person
  separation, restart, typed no-slot errors; controller load/retry/duplicate/dispose
  handling and proposal decoding.
- Full Flutter → Rust → Go → fixture-model → Rust → Flutter test: external transfer
  denied without consent, preference-backed proposal, deleted preference exclusion,
  fabricated slot rejection and no calendar/task mutation.

These are fixture results, not real model or live Calendar acceptance.

## Live setup

Follow [the gateway README](../../server/README.md) for the local dashboard and app
pairing. The legacy headless mode instead requires a shared environment token.
Provider credentials stay in the Go-side Keychain (legacy: Go environment).
Do not paste credentials into configuration JSON, chat,
model fields, committed files or logs.

[Local console validation](local-connections.md) records the additional paired-port
integration, secret-store checks and remaining native UI/OAuth gates.

Use synthetic events or the dedicated S1 test Calendar. Enter a configured target
ID in **When should I focus today?**. Review the target's configured provider/model
before enabling external transfer; local proxies can forward to external providers.

1. Save a preference, close/reopen the panel, then restart and verify the value/source.
2. Ask for focus time. Check date, interval, reason, source labels and freshness warning.
3. Edit/save the preference and ask again: old proposals disappear.
4. Delete the preference, restart and ask again: deleted value/evidence is absent.
5. Stop the provider/gateway and retry: show a typed error, never the old proposal.
6. Confirm no external or local Calendar event/task is created.

## Real-model evaluation — pending

At least three attempts per case; record build SHA, OS, gateway target/provider,
model version/quantization, offset, latency, typed outcome, interval validity,
evidence completeness and human reason-quality assessment. Exact text matching
and one successful answer do not establish quality.

| Case | Expected observation |
| --- | --- |
| Timed meetings + explicit window | Future interval fits preference and avoids loaded conflicts; grounded reason |
| Empty day, no preference | Defaults acknowledged; no invented memory/meetings; cache warning where applicable |
| Edited preference | New window/duration used |
| Deleted preference after restart | Old value absent from context, evidence and reasoning |
| Fully occupied/all-day | `no_focus_slot`, no model call |
| Midnight boundary UTC+09 | Correct interval, no overlap |
| Missing/stale/denied Calendar | No claim of complete current availability |
| Missing model/token/provider key | Recoverable `model_unavailable` |
| External permission unchecked | `external_transfer_denied`, no upstream call |
| Slow provider | Typed timeout within 45 seconds; no mutation |
| Malformed/tool-requesting output | `invalid_proposal`, no raw output or mutation |

Malformed/timeout behavior is deterministically covered by fixtures. Record actual
model behavior separately rather than calling fixture results live evaluation.

## Manual UI and dogfood — pending

Review desktop and 390px windows, large text, keyboard navigation and reduced
motion. Check scrolling, visible save/delete results, unsaved-edit protection,
pending-state controls, external permission and close-during-inference behavior.

After S1 and S2 verification, dogfood for three days with at least one explicit
request daily. Record usefulness, reason grounding, stale-data clarity and memory
editing/deletion. This does not replace the Personal Day MVP two-week dogfood.
