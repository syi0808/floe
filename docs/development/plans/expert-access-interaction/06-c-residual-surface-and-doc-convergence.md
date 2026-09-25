# Checkpoint 06-C — residual public surface and durable documentation convergence

- **Status:** reopened by 06-D residual audit.
- **Baseline:** 06-B completion.
- **Goal:** remove proven caller-zero transition helpers/exports and make durable architecture/product/ADR text match final source.
- **Exit:** 06-D is acceptance/archive only, not another semantic cleanup stage.

## 1. Caller-zero audit

06-D residual audit found `server/internal/application/calendar_admission_test.go` using `calendar.expert` as a positive admission consumer. Unlike the explicit legacy rejection fixtures, that success-path fixture must use the current Schedule package identity. This test-only correction belongs to 06-C; rerun the affected Go gate before returning to 06-D.

Do not delete by name alone.

### calendar_first_party_consumers

Current wrapper delegates to first_party_observe::calendar_policy consumers.

If only tests/old fixtures call it:
- update callers to canonical App policy owner;
- delete wrapper;
- remove comments implying Calendar owns a separate consumer policy.

### set_calendar_observe

Current search finds production helper plus native Calendar tests.

If product path no longer calls it:
- move tests to the same connection-level Observe owner operation used by product;
- delete helper and dead-code warning.

Keep apply_calendar_access/native subject/grant owner operations if current App orchestration uses them.

### resume helpers

Audit maybe_auto_resume, evaluate_auto_resume, claim_auto_child, worker/protocol callers and tests. Delete only caller-zero helpers. Keep canonical automatic-child production path.

### old failure names

AccessReviewRequired/ConsentRequired can remain as real low-level owner failures. Remove only alternate product flow mappings that bypass durable Conversation interaction.

### old wire fixtures

experts.calendar.* / calendar.expert may remain solely in explicit rejection/history fixtures. Make fixture purpose obvious. Do not re-add compatibility to erase test strings.

## 2. Narrow public/package surface

After caller migration:
- remove unused pub exports;
- reduce pub to pub(crate) where no real package caller remains;
- remove transition-only modules;
- remove Cargo dependencies whose only callers were deleted;
- update module-dependencies policy only when the physical edge is actually gone.

Do not add a new shared/facade crate to hide obsolete edges.

## 3. Machine assertions

Prefer existing regression tests. Add only cheap durable checks where a regression would otherwise be easy.

Prove:
- Schedule uses ordinary built-in registration.
- Registry remains source-independent.
- Access admission does not depend on built-in catalogue.
- Use with Floe remains projection/intent, not stored bool.
- DataAccessGrant/CalendarGrantPolicy contain no Registry identity.
- source revoke blocks later dependency admission.
- source/recipient blockers create durable interactions and original Run completes.
- linked interaction resume is fresh Run; budget Continue is exact pending-batch replay with 06-A provenance fence.
- only canonical ModelProvider/PreparedModelTransport production path remains.
- external transport depends on contextual Access consume, not saved consent.
- Observe/interactions never widen ActionAuthority.

## 4. Architecture docs

### docs/architecture/runtime.md

After 06-A/B:
- describe only canonical provider path; remove migration-era “legacy branch deleted after proof” prose.
- document budget Continue: exact validated batch/cursor + stored projection coverage reauthorization before execution and terminal release.
- keep Continue distinct from interaction Resume.

### docs/architecture/authority-recovery.md

Verify/add:
- recorded projection coverage is pending-batch authority;
- stale dependency blocks stored batch execution/release;
- saved pairing credential contains no recipient approval;
- external transport allow is request-scoped from consumed Access authority.

### docs/architecture/modules.md

Remove ModelTransport port from Inference description and any SourceHistoryBoundary concept from Conversation.

### docs/architecture/README.md

No checkpoint/progress wording.

## 5. Product docs

### docs/product/experience.md

Add the final user flow:

~~~text
blocked request
 -> Manager limitation
 -> inline Review request
 -> Not now or safe backend-projected action
 -> verified owner resolution
 -> linked follow-up in same conversation
~~~

Clarify:
- dismiss/Not now is not approval;
- source/processing review is not an Action proposal;
- original user text is not duplicated.

### docs/product/intelligence.md

Clarify:
- Experts declare/use semantic sources;
- Context acquires;
- Access authorizes the actual consumer;
- blocked source becomes typed no-conclusion/interaction, not Expert disappearance.

### integrations-and-privacy.md

Audit only; update if 06-B exposes stale saved-consent wording.

## 6. ADR/index audit

Do not rewrite history.

Audit decisions index and relevant ADRs:
- ADR 0027 is referenced as proposed/non-current. If its proposal is superseded by accepted 0028/0030/current architecture, make status/reference explicit.
- ADR 0028 remains connection/Observe rationale.
- ADR 0030 remains interaction/resume rationale.

Add a new ADR only for a genuinely new durable decision; applying existing invariants to remove transition machinery does not require one.

## 7. Stale comments/examples

~~~sh
rg -n 'legacy|temporary|transition|compat|vNext|SourceHistoryBoundary|ModelTransport|global consent|allow_external|external_recipients' README.md docs crates apps server
~~~

Classify every match:
- current external protocol;
- historical ADR;
- rejection fixture;
- stale text to delete/rewrite.

Examples must exercise current architecture or be deleted.

## 8. Gate

Run affected Rust/Flutter tests and:

~~~sh
cargo check --workspace --lib
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

06-C is complete only when 06-D has no semantic architecture cleanup left.
