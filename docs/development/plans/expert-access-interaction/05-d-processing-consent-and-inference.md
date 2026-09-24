# 05-D — exact-recipient consent and blocked model dispatch

- **Status:** planned; depends on 05-A through 05-C.
- **Exit:** eligible contextual consent can unblock an exact approved dispatch; unapproved or prohibited data never leaves via a fallback/explanation call.

## 1. Baseline and missing contract

Read together:

- `crates/modules/access/src/ports/model_dispatch.rs` — current synchronous recipient-string check lacks lineage/scope.
- `crates/modules/access/src/application/model_dispatch.rs:26-231` — admit/consume/revalidate and independent/dependent/unknown coverage checks.
- `crates/modules/inference/src/application/service.rs:76-285` — canonical executor, candidate planning, pre-budget admission, handoff and release.
- `crates/adapters/providers/src/control/recipient_authority.rs:1-71` — SavedConnectionRecipientAuthority still reads legacy saved consent values.
- `crates/app/src/vault_host/conversation_turn.rs` and common `expert_dispatch.rs:148-216` — root/Expert Inference composition.
- `crates/app/src/first_party_observe.rs:1-128` — LocalOnly and paired-source restrictions are not model-recipient approval.

Search all `allow_external`, `external_recipients`, `SavedConnectionRecipientAuthority`, `ModelDispatchRecipientAuthority`, `CanonicalModelRequest` and provider/server generation callers. The transport enforcement fields may remain; the saved global authority path may not.

## 2. Distinguish policy prohibition from missing eligible consent

Do not turn the existing `PolicyDenied` enum into a permission card. At Access dispatch admission, validate the current source/dependency and class restrictions sufficiently to know whether consent could safely authorize the requested route.

Recoverable: otherwise admissible dispatch with a specific non-secret candidate recipient/profile and missing contextual recipient consent.

Not recoverable by this consent: Credential/DeviceOnlyRaw, prohibited HighlySensitive transfer, Unknown coverage, forged/stale dependencies, LocalOnly-to-external, recipient different from a source's ApprovedRecipient, expired/foreign pairing, corrupt authority. Return a truthful limitation, but do not offer Allow that cannot make the dispatch legal.

**This slice does not rewrite source ProcessingRestriction or Observe grants.** LocalOnly remains LocalOnly after recipient approval. A future source processing-policy review would be a separate explicit Access decision. Preserve current Device-vs-ApprovedRecipient semantics; do not weaken those assertions as part of this feature.

## 3. Access owns contextual consent

Add a durable Access consent record/port with these bounded semantics:

```text
id, person_id, device/paired-connection binding
exact recipient identity + reviewed route/profile identity
origin intent lineage and authorized consumer/purpose set
reviewed input data classes
reviewed source scope entries (source/resource/category/grant-policy authorities), if dependent
revision/state, created_at, bounded expiry
```

An independent first model request may have no source entries; its review authorizes only that disclosed independent conversation scope. It does not approve future tool data. After fresh source acquisition adds scope, require a new explicit review unless that scope was already included in the reviewed grant. No wildcard Session/provider consent.

Consent must survive a crash between approval and linked resume. It is limited to this explicit intent lineage, not all future turns in the Session. Use an injected owner clock and a documented bounded TTL. Denial and expired/revoked consent never become permanent global choices.

Do not bind usable consent solely to a now-obsolete projection id: a fresh resume has new observations. Preserve original projection/attempt identity for audit; validate fresh projections against the reviewed exact source/resource/category/purpose/consumer and current authority bounds. New account, expanded resources, changed policy authority, route/recipient change or unrelated user request requires new consent.

Conversation stores a reference/semantic resolution receipt, not an independent authorization copy. Access may accept an opaque lineage binding supplied by admitted App execution, without depending on Conversation tables or allowing Flutter to select a lineage.

## 4. One typed model outcome across the canonical path

Introduce a typed recoverable model-admission outcome at the shared contract boundary used by ModelPort/InferenceExecutor. For example, `ModelCallOutcome::Ready(ModelResponse)` and `NeedsUserAction(ProcessingRequirement)` inside the outer Result. Choose one canonical type and migrate all root, built-in, Schedule, learner, provider-test and finalization callers; no parallel `generate_with_interactions` path or error-string side channel.

Inference derives the requirement from the actual selected candidate and Access decision. LLM output cannot name the authorized recipient. A pure contract carries bounded non-secret facts and trusted origin linkage; no transport/prepared-provider/credential value escapes.

When no model call was handed off, record a distinct durable blocked-attempt observation, not a fake successful ModelResponse or transport failure. Preserve prior dispatched-attempt usage. Admission denial never triggers hidden fallback to another recipient.

## 5. First-model and finalization blockage

A Manager cannot explain a denial by calling the very unapproved model. Conversation must handle the typed model blockage at first call and at finalization:

- publish a durable interaction under the exact attempted origin;
- commit a deterministic source-independent limitation from controlled copy;
- complete the original Run without fabricating model output or successful source coverage;
- preserve any previously settled work/usage and all required journal/cursor invariants.

If an approved model already can explain a source blocker, keep the normal Manager loop. Never invoke an otherwise forbidden route merely for a friendly error sentence. Hard failures and unknown post-dispatch outcomes are not converted into these expected completions.

For a blocked Expert model, report blocked domain judgment with the trusted ref to Manager. Background learner/non-conversation calls remain fail-closed without creating a fake Session or user prompt.

## 6. Dispatch and transport fences

Replace saved global consent as product authorization with current Access consent lookup, composed with current saved connection admission. Provider adapters still reload and verify current Person/device/connection at every relevant fence; removing consent fields must not remove credential/key identity validation.

Carry the Access-approved exact dispatch context through canonical Inference to the real provider boundary. Server `allow_external`/recipient transport fields, if required, are derived for this admitted request only, never from saved Settings state or client-provided free strings. Bind them to the selected route and prevent downstream silent rerouting.

Maintain three checks:

1. admission before new budget reservation/transmission;
2. current consent and dependency revalidation immediately before handoff;
3. revalidation before response release.

Consent revoked before handoff means zero outbound generation. Revoked after handoff means response suppression with usage still charged; do not claim transmission was recalled. Keep bounded provider response-loss/usage recovery semantics.

## 7. Acceptance matrix

- first Manager call lacks eligible recipient consent → zero generate calls, deterministic completed response + card;
- Allow matching independent scope → one linked run can call exact selected recipient;
- dependent input with source ApprovedRecipient matching requested recipient → explicit scoped consent then fresh admitted dispatch;
- LocalOnly or mismatched source recipient → still denied after consent, no Observe mutation;
- newly added source after independent consent → no silent scope expansion;
- root and delegated model paths use the same owner checks;
- fallback/profile change to another recipient → new review or denial, never reuse approval;
- revoke at admission/handoff/release → correct transfer counts and usage accounting;
- old saved allow_external=true → no authority;
- restart, foreign device/connection, changed source/grant/policy, expired consent → fail closed;
- learner/no Conversation lineage → no interaction creation and no consent bypass.

Use hermetic fake provider/server fixtures. Update the durable exact-recipient decision and architecture docs in this slice. Run Access/Inference/Agent/Conversation/provider tests and Go generation-contract tests if server is touched. No live OAuth/provider execution without separately explicit operator authorization.
