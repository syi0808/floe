# User-facing Memory settings

- **Status:** accepted
- **Date:** 2026-09-10
- **Scope:** S5 Personal Memory inspection and management
- **Related:** [ADR 0019](../../decisions/0019-governed-memory-and-playbook-learning.md)

## Product goal

Give people a normal settings experience for understanding and controlling what Floe
remembers. This is not a debug database viewer. It should answer four questions:

1. What does Floe currently remember about me?
2. Which proposed changes still need my review?
3. Why is a memory present and when was it learned?
4. How do I correct or forget it?

The view must preserve Floe's governed-memory boundary: only confirmed revisions are
shown as saved, pending candidates remain visually separate, and every edit or forget
operation remains source-, revision- and ledger-backed.

## Information architecture

`Settings → Data & privacy` gets a compact **Memory** card containing:

- saved-memory count;
- pending-review count, when non-zero;
- a short explanation that Floe uses only approved memories;
- a **Manage memory** action opening a dedicated settings page.

The dedicated **Memory** page uses this order:

1. **Pending changes** — shown only when non-empty, with Review actions.
2. **Saved memories** — searchable, with lightweight category filters.
3. **Memory controls** — use confirmed Memory and generate learning candidates.
4. **About memory** — privacy and lifecycle explanation.

The existing `Memory review` card moves into this page rather than remaining a second,
competing surface in Data & privacy.

## Saved-memory presentation

Each row shows user language first:

- statement;
- friendly category: Preference, Fact, Commitment, or Other;
- `Added` or `Updated` date;
- optional `Needs attention` state for stale items.

Observation and Inference map to **Other** in the primary filter. Technical epistemic
status, confidence and IDs do not appear in the list. They remain available in an
expanded **Details** section for transparency and support.

Selecting a row opens a detail sheet/page with:

- full statement and category;
- active revision and validity period;
- whether it was explicitly provided or inferred;
- source count and safe source labels;
- created/updated time;
- **Edit** and **Forget** actions;
- an expandable technical section with revision, confidence and actor.

Raw conversation text is never displayed in the list. `View source` is a separate,
explicit action and must load a bounded, still-authorized source projection. Hidden
reasoning, model prompts, credentials and highly sensitive raw evidence are never
shown.

## Pending changes

Pending candidates keep the existing Approve/Reject behavior, but use the same Memory
row language and add:

- create/revise/forget labels;
- before/after presentation for revisions;
- source count and proposed date;
- `Inspect source` when the source is still available.

Approval refreshes both sections atomically from the committed result. Rejection
removes the proposal without changing Saved memories. Pending proposals never appear
in the saved count or model context.

## User mutations

### Edit

The user edits the statement, not internal confidence or actor fields. The host creates
an explicit user revision against `target_id + expected_revision`, preserves the prior
revision as rollback material, and records the User actor and trusted time. A stale
revision returns conflict and reloads the latest value instead of overwriting it.

For the first version, category is preserved. An explicit edit upgrades epistemic
status to Fact and confidence to 1000 while retaining provenance to the prior source
and the user mutation.

### Forget

Forget requires a confirmation explaining that the item stops influencing future
answers immediately. The transaction writes a tombstone/denial entry before deletion
propagation. The item disappears from Saved memories and retrieval even if archive or
derived cleanup must continue asynchronously.

Forget is not implemented as candidate rejection, hard database deletion, or a local
UI hide. Re-learning checks the tombstone so restart, compaction and extractor upgrades
cannot resurrect the same fact.

## Controls

| Control | Default | Behavior |
| --- | --- | --- |
| Use saved memories | On | Allows eligible active revisions in future context |
| Suggest new memories | On | Allows deterministic discovery and staged Learner proposals |

Turning off **Use saved memories** does not delete anything. Turning off **Suggest new
memories** stops new discovery/queue work but does not reject existing pending items.
The mandatory Review gate is not exposed as a disable-able control.

Background Learner scheduling is an implementation detail and should not be presented
as an AI/model toggle. A debug build may expose queue diagnostics elsewhere, but they
are not part of the user-facing Memory page.

## Contract design

Add a separate bounded management contract instead of reusing retrieval context:

```text
MemoryOverview {
  personId,
  controls,
  savedCount,
  pendingCount,
  memories: MemorySummary[],
  pending: MemoryCandidateSummary[],
  nextCursor?
}

MemorySummary {
  targetId,
  revision,
  statement,
  kind,
  epistemicStatus,
  confidenceMillis,
  state,
  sourceCount,
  createdBy,
  createdAt,
  validFrom?,
  validUntil?
}

MemoryMutation =
  revise { targetId, expectedRevision, statement }
  | forget { targetId, expectedRevision }
  | setControls { expectedRevision, useSaved, suggestNew }
```

The overview returns active/stale user-manageable Memory only and is capped/paginated;
it does not reuse `personal_memory_context`, whose temporal and relevance filtering is
for model retrieval. Archived, tombstoned and superseded revisions are loaded only by
an explicit history/detail request.

The caller supplies identifiers, expected revisions and user-entered text only. Core
owns Person scope, actor, decision time, hashes, mutation records and tombstones.

## States and accessibility

- **Locked:** explain that private data must be unlocked; do not retain the previous
  Person's Memory in controller state.
- **Empty:** explain how explicit remember requests create review proposals.
- **Loading/error:** keep Saved and Pending failures independently retryable.
- **Search/filter empty:** preserve the search field and offer `Clear filters`.
- All actions have text labels, keyboard focus, semantic descriptions and confirmation
  copy that does not rely on color.

## Implementation increments

1. **Complete:** add bounded active-Memory overview/query DTOs and encrypted-vault tests.
2. **Complete:** add controller projections and the dedicated read-only Memory page.
3. Move pending Review into the page and add before/after/source metadata.
4. Add revision-checked explicit user Edit with mutation history.
5. Add tombstone-first Forget and deletion propagation.
6. Add persisted controls, search/pagination and accessibility/golden coverage.

The first increment deliberately exposes no mutation until edit/forget transactions
and their recovery tests are complete.

## Acceptance checks

- A locked or different Person cannot observe cached Memory.
- Pending/rejected/tombstoned revisions never appear as saved or enter retrieval.
- Approve, edit and forget refresh the committed server result without optimistic fake
  state.
- Concurrent edit/forget uses revision CAS and cannot overwrite a newer decision.
- Forget excludes retrieval before asynchronous cleanup and survives restart.
- The default page contains no model prompt, hidden reasoning, raw database payload or
  queue implementation detail.
- Empty, locked, loading, error, populated, pending and conflict states have widget
  coverage; Core/FFI contracts have malformed, cross-Person and key-loss coverage.
