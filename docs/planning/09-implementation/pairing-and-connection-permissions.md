# Pairing and connection-scoped permissions implementation plan

- **Status:** active
- **Date:** 2026-09-13
- **Decision:** [ADR 0028](../../decisions/0028-pairing-integrated-authority-and-connection-permissions.md)

## Objective

Replace protocol-shaped authority enrollment UI with one pairing ceremony, then manage connector
data grants on the connection that owns them. Deliver macOS Calendar as the first complete connection
detail: EventKit system access, selected calendars and consumer grants remain distinct but are managed
together. Pairing must create no connector grants.

## Shared pairing contract

The first implementation uses the existing strict enrollment challenge as the proof primitive while
moving it inside pairing.

1. The unlocked local vault prepares one owner issuer key before pairing begins.
2. `POST /pair/start` receives schema version, Person, device, issuer key ID and issuer public key.
3. The response returns pairing ID, comparison code, polling proof, expiry, signed server producer
   identity, issuer fingerprint and a producer-signed owner challenge bound to the pending pairing.
4. The client verifies the producer and challenge, signs it with the prepared issuer and submits
   `POST /pair/confirm` with pairing ID, polling proof, challenge ID, key ID and signature.
5. Dashboard approval names the exact pairing ID and issuer fingerprint and is unavailable before
   local confirmation.
6. `POST /pair/poll` returns no credential until client credential, producer binding and trusted issuer
   are durably active. Its states are `pending`, `local_confirmed`, `approved`, `rejected`, `expired`
   or `repair_required`.

Wire decoders reject unknown or duplicate fields, wrong identities, changed keys, invalid signatures,
expired attempts and replay. Existing bearer-based enrollment routes may remain temporarily for tests
and migration, but the product UI must not invoke them after cutover.

The server owns the durable activation transaction. `Clients`, `TrustedIssuers` and the producer
binding are written as one state transition before a bearer is exposed. A failed or uncertain write
latches protected authority unavailable; it must never yield a client-only usable pairing. The client
saves its credential and producer pin only after verifying an approved response. Since server state,
Keychain and the local vault cannot share a transaction, retries use the stable pairing ID and exact
intent rather than generating replacement trust.

## Delegated work units

### P1 — Go pairing and atomic trust activation

**Owner:** one Go/server agent. **Dependencies:** none. **Parallel with:** P3.

- Extend pending pairing state and strict request/response models with issuer and producer bindings.
- Generate the producer-signed enrollment challenge during pairing and add `/pair/confirm`.
- Add one durable activation path for paired client and trusted issuer; expose the token only afterward.
- Require local confirmation and exact issuer fingerprint in dashboard approval.
- Replace the dashboard's routine enrollment queue with pairing identity details; keep active trust
  revocation available.
- Preserve client deletion, issuer revocation, trust quarantine and protected-route checks.

**Acceptance:** focused authorization and console tests cover ordering, expiry, replay, mismatch,
save failure, restart, revoke/delete and zero grant creation. Then run `go test ./...` and the affected
authorization/console race suites from `server/`.

### P2 — Rust owner proof and pairing lifecycle

**Owner:** one Rust/native agent. **Dependencies:** P1 wire shapes. **Parallel with:** late P3.

- Add typed prepare/confirm/status DTOs and strict transport decoding for the shared contract.
- Reuse the encrypted owner issuer key and strict producer-signed challenge validation; do not add an
  opaque signing API.
- Delay durable producer pinning until approved pairing finalization and make exact retries idempotent.
- Expose bounded FFI operations for pairing preparation/confirmation/finalization and retire routine
  enrollment operations from the client-facing path.
- Keep existing admission and release validation bound to the approved pairing identities.

**Acceptance:** Core, infra, protocol and FFI tests cover wrong Person/device/key, changed producer,
replay, cancellation, restart, pin timing, approved recovery and no protected network use before
pairing. Run focused package tests, formatting and Clippy for touched crates.

### P3 — macOS Calendar connection permission detail

**Owner:** one Flutter/client agent. **Dependencies:** none for the first vertical. **Parallel with:** P1.

- Compose EventKit authorization, selected calendars and existing Calendar consumer grants in
  `Connections → macOS Calendar`.
- Move `AgentCalendarSettings` and its source-change coordination out of **Data & privacy**.
- Keep grant resources bounded to selected calendars; selecting more calendars never expands a grant.
- Leave Android Calendar management in place and retain non-connection privacy controls.
- Make missing Calendar access recovery navigate to the macOS Calendar connection detail.

**Acceptance:** focused widget/controller tests directly exercise all three permission layers,
navigation, unavailable/revoked states and non-expansion. Run the affected Flutter tests and
`flutter analyze`.

### P4 — Flutter integrated server pairing

**Owner:** the Flutter/client agent after P2. **Dependencies:** P1 and P2.

- Orchestrate issuer preparation, `/pair/start`, local confirmation, approval polling and final pin/save
  behind the existing **Pair server** experience.
- Show comparison code and ordinary pending/error states; put fingerprints behind optional security
  details and use **Pair again** for identity repair.
- Remove `Server authority enrollment` and its inspect/enroll/status actions from Remote server settings.
- Verify pairing success leaves every connector permission Off.

**Acceptance:** focused pairing tests cover success, cancel, expiry, rejection, mismatched producer,
vault lock, server save failure and retry without a second trust intent.

### P5 — Remote connection-scoped grants

**Owner:** the Flutter/client agent after P4. **Dependencies:** P2 and existing grant APIs.

- Move Calendar and remote-view grant preview/review/pause into the matching server connection detail.
- Filter all resource choices and mutations by the displayed stable `connection_id`.
- Remove server-wide grant selection from Remote server settings.
- Keep a non-expanding global permission summary/navigation surface; do not add a second grant editor.

**Acceptance:** two same-provider fixture connections remain independent; detail screens cannot mutate
another connection; disconnect/pause blocks later use; the global overview cannot expand scope.

## Integration order

1. Land P1 and freeze the tested wire examples.
2. Land P3 independently while P2 adopts the frozen contract.
3. Rebase and land P2 against P1 fixtures.
4. Land P4, then exercise pairing end to end with the real local server and unlocked vault.
5. Land P5 and exercise one macOS Calendar grant and one server connection grant end to end.
6. Add regression tests for defects found during direct exercise and update validation evidence.

Each unit is one reviewable commit unless a test-only correction is inseparable from its implementation.
Agents must not weaken authorization assertions, infer old pairings as issuer approval or expand Android
scope. Unrelated failures are reported rather than fixed.

## Final acceptance

- A fresh local state completes one visible pairing ceremony and survives server/app restart.
- Pairing produces an active exact issuer and no data grant; identity rotation fails closed.
- macOS Calendar permissions are edited only from its connection detail and directly exercised there.
- Remote grants are edited only from their owning connection; same-provider accounts do not mix.
- Data & privacy and any global Permissions view summarize or navigate without expanding access.
- Pair, grant, use, pause, revoke, disconnect, identity change and re-pair are directly exercised on
  macOS, with unverified iPhone/iPad behavior reported explicitly.
