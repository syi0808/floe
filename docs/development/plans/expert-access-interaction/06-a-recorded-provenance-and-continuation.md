# Checkpoint 06-A — recorded provenance and budget-continuation convergence

- **Status:** next.
- **Baseline:** c137b35bf44ea12b1d2f5aa57b2f86aa6d0f1761.
- **Goal:** make recorded DependencyCoverage the only authority for historical and pending-batch reuse, then delete message-shape source-history inference.
- **Exit:** no SourceHistoryBoundary production surface remains and stale coverage can neither execute nor release a resumed pending batch.

## 1. Current anchors

Already canonical:

- crates/contracts/agent/src/ports.rs — ValidatedModelBatch.projection_coverage.
- crates/modules/conversation/src/application/history_projection.rs — project_model_conversation_history.
- crates/modules/context/src/application/projection.rs — project_coverage.
- crates/modules/context/src/application/model_coverage.rs — current dependency authorization behavior.

Transitional:

- crates/contracts/agent/src/expert_model.rs — SourceHistoryBoundary.
- crates/modules/conversation/src/turn/source_history.rs — ConservativeSourceHistoryBoundary, carries_source_history, bounded_source_history_start.
- crates/modules/conversation/src/application/admission.rs — TurnPreparationRequest.boundary and continuation StaleContext heuristic.
- crates/modules/conversation/src/application/history_projection.rs — narrow_by_source_boundary.
- crates/app/src/vault_host/conversation_turn.rs — ConservativeSourceHistoryBoundary wiring.

## 2. Replace continuation heuristic with exact batch coverage

TurnMode::Continue is budget/deadline continuation. It may carry a ContinuationSnapshot with pending_batch + cursor and must continue the exact validated batch without model recall.

### 2.1 One Context-owned strict coverage gate

Reuse DependencyResolver/project_coverage. Add a bounded operation with these semantics:

~~~text
authorize_recorded_coverage(coverage)
  Independent -> current
  Unknown -> stale / fail closed
  Dependent -> every dependency must authorize now
~~~

Expected policy denial means stale derived work. Storage/provider/native infrastructure errors remain hard failures.

Do not make Conversation implement per-source grant logic.

### 2.2 Pre-execution gate

Before Conversation supplies EngineResumeState:
- read pending_batch.projection_coverage;
- authorize it through the same current resolver used by Context history/model dispatch;
- only on success hand the exact batch/cursor to Engine.

No stored Tool, Delegation, Preamble or Answer step may execute first.

The check does not replace each Tool/Delegation owner's own current admission or replay rules.

### 2.3 Pre-release gate

A source may revoke after the first continuation check.

The Engine already reports answering_projection_coverage from the persisted batch when a stored Answer executes.

Before Conversation commits/releases that terminal answer:
- reauthorize the exact reported coverage;
- if stale, suppress the derived answer and terminate safely;
- preserve usage/journal truth;
- do not model-recall or mark the answer Independent.

This race test is mandatory.

### 2.4 No-pending-batch continuation

If a continuation has no pending validated batch, the next ordinary model projection reauthorizes history. Do not add duplicate policy gates without a proven need.

## 3. Delete heuristic surface

After C01–C08 tests pass:

Contracts:
- delete SourceHistoryBoundary trait;
- remove public export.

Conversation:
- delete turn/source_history.rs;
- remove ConservativeSourceHistoryBoundary;
- remove carries_source_history;
- remove bounded_source_history_start;
- remove narrow_by_source_boundary and export;
- remove TurnPreparationRequest.boundary;
- remove message-shape continuation rejection.

App:
- remove boundary injection from root turn preparation.

Do not replace them with another string/id classifier.

## 4. Archive/compaction safety

Verify compaction/archive relies on recorded coverage and cannot reintroduce stale derived data.

If a compaction summary lacks sufficient durable coverage to answer that question, fix the owning coverage record before deleting the fallback. Do not classify summaries by message kind as the final design.

Pending interactions whose origin is archived remain retrievable per ADR 0030.

## 5. Required scenarios

C01 Independent pending batch:
- continue exact batch/cursor;
- no model recall.

C02 current dependent pending batch:
- current dependency reauthorizes;
- exact batch runs;
- answer keeps exact coverage.

C03 revoke before continuation:
- no stored step executes;
- no new Tool/Delegation intent;
- no answer release.

C04 revoke after initial gate, before terminal release:
- stored Answer is not committed/released;
- no model recall.

C05 Unknown coverage:
- cannot authorize derived continuation.

C06 multi-source coverage:
- all current -> continue;
- either dependency stale -> no step executes;
- no aggregate synthetic dependency.

C07 normal historical projection:
- current recorded coverage retained;
- stale recorded coverage removed;
- historical User text retained;
- no message-id classifier.

C08 compaction/archive:
- revoked dependency cannot re-enter model input through archive/summary.

## 6. Residual search

~~~sh
rg -n 'SourceHistoryBoundary|ConservativeSourceHistoryBoundary|narrow_by_source_boundary|carries_source_history|bounded_source_history_start' crates apps
~~~

Expected: zero.

Confirm canonical provenance remains:

~~~sh
rg -n 'projection_coverage|read_turn_coverage|project_model_conversation_history|project_coverage' crates
~~~

## 7. Gate

Run focused agent-contract, Context, Conversation, Vault Conversation recovery and App Conversation tests, then:

~~~sh
cargo check --workspace --lib
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Commit only after C01–C08 directly pass.
