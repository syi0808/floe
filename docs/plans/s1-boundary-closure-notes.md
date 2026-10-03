# S1 boundary closure and deferred verification

Updated 2026-10-02 17:43 UTC. This records implementation decisions and future validation obligations. It is not evidence of a passing build or test.

## Implemented boundaries

- Conversation owns durable command admission, background execution, cancellation, events, blocked review publication, review resolution and fresh resume. App exposes a verified actor and typed generation handles. The old Conversation/source/pairing App worker variants and duplicate event buffer are removed.
- Public Session and Interaction projections are owner values. Session views contain at most 256 recent messages, safe task/artifact summaries, opaque continuation, usage and explicit unknown charged cost. They exclude raw storage scope, model choices, dependency authority and artifact payloads. FFI performs field and enum conversion.
- A blocked Run stores actual Conversation interaction references in the same transaction as its group and publication. Access review references cannot substitute for interaction IDs. Owner Run projections define blocked, partial and indeterminate report semantics.
- Projection review preparation replays exact persisted Access review origins before any fresh source observation. Source configuration first invalidates all non-revoked grants through the Access receipt protocol. Actions defines the pure invalidation transition; Vault supplies atomic persistence, leaving Executing/Unknown action states untouched.
- Native-only integration reviews use a Device target. Remote reviews use a Gateway target with exact revision. Product DTOs never manufacture a Gateway for local permission setup.
- Native callback traffic uses a separate mechanical lane with the same LocalContextHost, verified caller and host admission generation. This is necessary because direct source review/configuration calls await native metadata and the product FFI lane is serial. The native lane accepts only register/poll/complete/fail/dispose. It cannot issue product commands or select another actor.
- Host closing rejects new admissions, closes exact native registrations to wake broker waiters, drains admitted calls, shuts down owner tasks and seals the Vault generation. A retained native lane owns no raw core pointer. A stale lane release cannot dispose a replacement registration.
- Device identity and existing-profile selection are explicit. Malformed/unreadable existing profiles fail; source refactoring does not reset, migrate, overwrite, or silently create a replacement profile.

## S1/S2 sequencing

G1 qualifies the complete Conversation/Gateway/source-review vertical after all caller integration. Full Day refresh, multi-source acquisition/mirror and complete Actions owner extraction remain S2. Cached Day reads and CRUD remain; refresh controls are absent until real owner implementation. Prepared Day DTOs do not imply runtime availability. No synthetic source IDs, first-source selection, compatibility publication path, or false-success stub is accepted.

## Required S3 behavior reconstruction additions

1. Native callback completes while the product lane is waiting; callback traffic makes progress independently.
2. Core close while metadata is outstanding closes exact registrations, wakes waiters, drains active calls and terminates without an unbounded queue deadlock.
3. Lane retained after core release rejects new calls without dereferencing freed core memory.
4. Lane release with native waiters interrupts them. Repeated logical close is idempotent; FFI allocation is freed exactly once after its final call.
5. Registration replacement followed by stale lane release leaves the replacement intact.
6. Wrong-family/epoch/request completion, late completion after host close, duplicate completion, timeout and cancellation fail with truthful owner outcomes.
7. Product commands and caller-supplied person/device authority are rejected on the mechanical lane.
8. Source review lost-response replay returns original stored review references even after source facts change; a changed intent under the same command conflicts.
9. Exact pairing-start proof binds private receipt replay. A public operation identifier alone cannot retrieve a polling credential. Lost response, cancel/activate race and restart recovery never imply a fresh pairing.
10. Continuation retains unknown charged tokens and cost; immutable journal receipts and aggregate uncertainty never become fabricated observed usage.
11. Every blocked report carries actual interaction IDs, produces no final message, and fresh resume reuses the original user message exactly once after whole-group resolution.
12. Safe session wire omits all internal authority; timed-out and interrupted Task states remain distinct. Navigation cards carry display context, never physical source authority.

## Verification state

No formatting, compilation, package build, test execution or architecture checker has run during T0/S1 preparation. Only source reading, inventories, source/diff comparison and dependency reasoning have been used. G1 remains pending until all first-slice owners and callers are closed. Apple compiler coverage will be coordinated separately on the authorized Mac; full application builds remain G2.
