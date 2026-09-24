# Checkpoint 04-E — Settings, external processing, and Act separation

- **Execution baseline:** 04-D completion
- **Depends on:** canonical connection-level Observe UI for native and remote sources
- **Goal:** remove duplicate/global source permission ceremonies while preserving exact external-processing and Action authority.

---

## 1. Data & privacy target

Settings is for cross-connection concerns.

After 04-E, Data & privacy may contain:

- memory/data retention controls;
- privacy explanation;
- navigation to Connections;
- other genuinely global privacy controls.

It must not be the primary editor for:

- Calendar Observe;
- Contacts Observe;
- Attention Observe;
- Wellbeing Observe;
- remote connector Observe;
- source resource selection.

Those belong to connection detail.

---

## 2. Remove generic external-model toggle

Baseline _AiProcessing edits:

~~~text
ServerConnection.allowExternal
"Allow external model providers"
~~~

Remove this generic Settings control.

Do not delete or bypass enforcement that checks exact-recipient consent.

Do not automatically set allowExternal=true on connection or Observe enable.

Preserve stored authority if Inference still needs it for current profiles.

Fresh/default false remains false until future contextual consent flow in Checkpoint 05 changes it.

If removing UI makes a fresh remote-model path unavailable until Checkpoint 05, record that as intentional fail-closed boundary rather than weakening consent.

---

## 3. Do not conflate source transport and model processing

Review all "processing" concepts around remote source grants.

Connection detail Use with Floe means:

> Floe may read this source under its current first-party Observe policy.

It does not mean:

> Any remote model provider may receive this source data.

Do not display one status combining both questions.

Checkpoint 05 will surface recipient-specific decision when a real run needs one.

---

## 4. Action permissions remain separate

ActionPermissionsSection is not duplicate Observe state.

Keep current authority owner unless another concrete stale path is found.

Required regression:

~~~text
Use with Floe off/on
  -> Calendar create ActionAuthority unchanged
~~~

Disconnected source may make an action impossible through provider/system preconditions, but stored Act policy is not rewritten to mirror Observe.

Connection detail may explain that actions have separate permissions but should not mirror ActionAuthority into source switch.

---

## 5. Remove duplicate source Settings controls

After 04-C:

- remove AgentPersonalAccessSettings from Settings entry points;
- move reusable classes to Connections;
- delete dead Settings-only source widgets/tests;
- remove Android source editor blocks if they only exist as duplicate Settings path; do not build replacement Android parity.

Do not remove OS permission adapters or dormant Android source code merely because Settings UI is gone.

---

## 6. Settings navigation and copy

Replace fragmented source permission copy with concise cross-connection explanation.

Intended meaning:

- Sources are managed from Connections.
- External actions use separate Action permissions.
- External model processing may require approval when needed.

Avoid presenting "LLM access" as a global source yes/no.

---

## 7. Wire/state cleanup

Search product-only external-processing setter calls after UI deletion.

If updateModelConsent / allow_external has no legitimate product caller until Checkpoint 05:

- keep owner/storage enforcement code if needed for recipient authorization;
- remove only obsolete Settings-specific plumbing;
- do not delete durable consent record merely because current editor is gone.

No compatibility alias.

---

## 8. Tests

### Settings

- no "Allow external model providers" toggle;
- no source Observe editor;
- Connections navigation available;
- memory/global privacy controls still render.

### External processing

- connection + Observe Active with recipient consent false -> remote model release still denied/consent-required;
- toggling Use with Floe does not change recipient consent;
- existing true recipient consent persists across source pause/on unless current authority semantics independently invalidate it.

### Act

- Observe on/off leaves ActionAuthority unchanged;
- Action permissions UI still works;
- provider/system preflight can block write independently.

---

## 9. Residual gate

~~~sh
rg -n 'Allow external model providers|_AiProcessing|AgentPersonalAccessSettings|LLM access' apps/client/lib/features/settings apps/client/test/features/settings
~~~

Expected no source/LLM toggle or source permission editor.

~~~sh
rg -n 'allowExternal|allow_external|updateModelConsent|requiresExternalConsent|coversExternalRecipient' apps/client crates server
~~~

Classify remaining matches:

- current recipient authority/enforcement: keep;
- obsolete Settings plumbing: delete;
- source Observe shortcut: forbidden.

---

## 10. Verification

~~~sh
cd apps/client
flutter analyze
flutter test

cd ../..
cargo test -p floe-inference
cargo test -p floe-app
cargo test -p floe-protocol
git diff --check
~~~

Do not start 04-F while Settings still has a second source permission editor or while source connection changes mutate external-recipient or Action authority.
