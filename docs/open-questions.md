# Open questions

**Status:** active decision backlog  
**Last updated:** 2026-09-02

## Selected baseline; validate rather than reopen by default

- **Client UI:** Flutter/Dart is the main cross-platform visual surface.
- **Shared core:** Rust owns canonical local domain mutations, policy, crypto, sync primitives, and local persistence orchestration.
- **Platform bridges:** Swift owns Apple API surfaces; Kotlin owns Android lifecycle/API surfaces; Windows low-level integration is Rust + `windows-rs` by default.
- **Server control plane:** Go.
- **Connectors:** native-first. Device connectors use Rust Core + native adapters; always-online SaaS connectors use Go. Node/TypeScript is not in the default runtime.
- **Portable integrations:** evaluate a declarative ConnectorSpec for standard OAuth/REST/pagination/webhook patterns; Activepieces is a build-time corpus/reference, not a shipped runtime dependency.
- **Persistence foundation:** Turso, with the Rust Core owning embedded device storage.
- **Extensions:** Experts share a semantic contract. Declarative Experts are default; code Experts are capability-scoped Wasm candidates for desktop/server. Arbitrary third-party mobile code and arbitrary plugin UI are excluded.
- **Repository:** monorepo first.

The detailed source of truth is `docs/planning/`, including decisions D-017 onward in `08-engineering/decisions.md`.

## Resolve before the first implementation commitment

1. **MVP input boundary:** manual entry only, or read-only calendar import from the beginning?
2. **Now/Next policy:** what time horizon and ranking rules work in actual dogfooding?
3. **Action confirmation:** which actions, if any, may use a remembered confirmation preference?
4. **Initial project slice:** exact domain schema and command/snapshot contract for Event, Task, Note, Capture, TimelineProjection, and ActionProposal.
5. **FFI and IPC contract:** generated C ABI and batched snapshots for Flutter ↔ Rust; desktop local IPC transport remains a PoC decision.

## Required PoCs

- Wake word, streaming transcription, idle CPU/battery, and local-audio boundary.
- Health-derived state without exporting raw health data.
- ConnectorSpec portability, source importing, authorization mapping, retention, and calendar-route deduplication.
- Memory claim/evidence schema, identity-merge threshold, sensitive-memory policy, and deletion propagation.
- Turso embedded build feasibility, encryption, two-device conflict/deletion behavior, vector retrieval, and self-host integration.
- Expert semantic contract, permission projection, private state, audit records, declarative runtime, and Wasm sandbox resource limits.
- Multi-device authorization, device revocation, and encryption key hierarchy.

## Deferred by intent

- Full people/relationship memory experience.
- Marketplace discovery and commerce; local/private Expert packages and the runtime contract stabilize first.
- External task/note connectors: Floe Tasks and Notes remain canonical initially.
- Multi-platform feature rollout, ambient behavior, hosted service operations, and managed OAuth.

The linked ChatGPT conversation remains unimported because it was inaccessible from the current environment; this document contains only the supplied v0.8 planning decisions.