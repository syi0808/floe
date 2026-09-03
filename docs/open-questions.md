# Open questions

**Status:** active decision backlog  
**Last updated:** 2026-09-03

## Selected baseline; validate rather than reopen by default

- **Primary Day Canvas:** calendar-first. The time grid is the main surface; Tasks, Notes, and Floe are layered according to their role rather than rendered as an equal-weight feed.
- **Now / Next:** current-time line, current-event emphasis, and optional compact next-event navigation. A large permanent Now/Next hero is not the target design.
- **Client UI:** Flutter/Dart is the main cross-platform visual surface.
- **Shared core:** Rust owns canonical local domain mutations, policy, crypto, sync primitives, and local persistence orchestration.
- **Platform bridges:** Swift owns Apple API surfaces; Kotlin owns Android lifecycle/API surfaces; Windows low-level integration is Rust + `windows-rs` by default.
- **Server control plane:** Go.
- **Connectors:** native-first. Device connectors use Rust Core + native adapters; always-online SaaS connectors use Go. Node/TypeScript is not in the default runtime.
- **Portable integrations:** evaluate a declarative ConnectorSpec for standard OAuth/REST/pagination/webhook patterns; Activepieces is a build-time corpus/reference, not a shipped runtime dependency.
- **Persistence foundation:** Turso, with the Rust Core owning embedded device storage.
- **Extensions:** Experts share a semantic contract. Declarative Experts are default; code Experts are capability-scoped Wasm candidates for desktop/server. Arbitrary third-party mobile code and arbitrary plugin UI are excluded.
- **Repository:** monorepo first.

The detailed source of truth is `docs/planning/` and root `DESIGN.md`.

## Day Canvas decisions still requiring dogfood

The product direction is calendar-first, but these details remain intentionally unresolved:

1. **Calendar geometry:** default hour height, zoom range, short-event treatment, and overlap policy.
2. **Scheduled Task semantics:** exact domain model for planned execution time; deadline must remain separate.
3. **Today rail:** default width, open/closed state, and whether Tasks/Notes share one rail or switchable sections.
4. **Notes:** when a Note is a Today Note versus an annotation linked to Event/Task/Person context.
5. **Floe proposals:** exact ghost-block visual and accept/dismiss/details interaction.
6. **Mobile composition:** how Tasks/Notes move into sheets or secondary surfaces while keeping the calendar mental model.
7. **Dense days:** folding/zoom behavior before reducing typography or target size.
8. **Calendar source color:** how much provider/source tint improves scanning without making the UI noisy.

## Resolve before first real Calendar import

1. **Timezone representation:** adopt IANA timezone identity rather than fixed UTC offsets for canonical calendar semantics.
2. **External source provenance:** define provider/connection/resource/external ID/revision fields before connector data enters canonical projections.
3. **Range-query storage:** expose/index hot calendar fields so Day Canvas does not deserialize a Person's complete Event history on every load.
4. **Atomic revision writes:** move from read-check-write to storage-level conditional mutation before multiple mutation sources exist.
5. **Source route deduplication:** decide how direct Google/Microsoft connectors suppress duplicate OS-calendar ingestion.
6. **All-day and recurrence semantics:** preserve provider behavior before editable external calendars are attempted.

## Universal Capture questions

The current explicit Event/Task/Note classification is a trustworthy vertical-slice mechanism, not necessarily the final interaction.

Resolve:

- how Pending Captures are reviewed/recovered;
- when Floe may suggest classification;
- how much metadata can be deferred;
- whether a quick capture can appear temporarily in Today context before canonical classification;
- how correction changes provenance and derived candidates.

## Action and assistant questions

1. Which actions, if any, may use a remembered confirmation preference?
2. How should a minimal Manager attach suggestions to the calendar without becoming a permanent insight feed?
3. What evidence/rationale is shown when a Floe proposal comes from Health, Memory, or an Expert?
4. What intervention budget works during real dogfood?

## Required PoCs

- Calendar-first Day Canvas with real calendar data and dense-day scenarios.
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
- Full Month-view calendar replacement.
- Multi-platform feature rollout, ambient behavior, hosted service operations, and managed OAuth.
