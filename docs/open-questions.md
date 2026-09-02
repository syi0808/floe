# Open questions

**Status:** active decision backlog  
**Last updated:** 2026-09-02

## Selected baseline; validate rather than reopen by default

- **Client UI:** Flutter/Dart is the main cross-platform visual surface.
- **Shared core:** Rust owns canonical local domain mutations, policy, crypto, sync primitives, and local persistence orchestration.
- **Platform bridges:** Swift owns Apple API surfaces; Kotlin owns Android lifecycle/API surfaces; Windows low-level integration is Rust + `windows-rs` by default.
- **Server control plane:** Go.
- **Connector host:** isolated TypeScript/Node.js runtime.
- **Persistence foundation:** Turso, with the Rust Core owning embedded device storage.
- **Repository:** monorepo first.

These are recommended v0.5 baselines. Their source of truth is `docs/planning/09-implementation/` and decisions D-017 through D-025 in `docs/planning/08-engineering/decisions.md`.

## Resolve before the first implementation commitment

1. **MVP input boundary:** manual entry only, or read-only calendar import from the beginning?
2. **Now/Next policy:** what time horizon and ranking rules work in actual dogfooding?
3. **Action confirmation:** which actions, if any, may use a remembered confirmation preference?
4. **Initial project slice:** exact domain schema and command/snapshot contract for Event, Task, Note, Capture, TimelineProjection, and ActionProposal.
5. **FFI and IPC contract:** generated C ABI and batched snapshots for Flutter ↔ Rust; desktop local IPC transport remains a PoC decision.

## Required PoCs

- Wake word, streaming transcription, idle CPU/battery, and local-audio boundary.
- Health-derived state without exporting raw health data.
- Connector contract, Activepieces adapter cost, authorization mapping, and sync semantics.
- Memory claim/evidence schema, identity-merge threshold, sensitive-memory policy, and deletion propagation.
- Turso embedded build feasibility, encryption, two-device conflict/deletion behavior, vector retrieval, and self-host integration.
- Multi-device authorization, device revocation, and encryption key hierarchy.

## Deferred by intent

- Full people/relationship memory experience.
- Connected-data experts and provider integrations.
- Multi-platform feature rollout and ambient behavior.
- Hosted service operations and managed OAuth.

The linked ChatGPT conversation remains unimported because it was inaccessible from the current environment; this document contains only the supplied v0.5 planning decisions.