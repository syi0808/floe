# Checkpoint 06-B — legacy model transport and saved-consent deletion

- **Status:** blocked on 06-A.
- **Baseline:** 06-A completion.
- **Goal:** leave one production model provider path and remove obsolete saved global-recipient consent while preserving the server request-scoped transfer fence.
- **Exit:** no legacy ModelTransport branch or saved recipient-consent state remains.

## 1. Keep/delete distinction

Keep:
- Access contextual exact-recipient consent;
- ContextualRecipientAuthority;
- ModelDispatch admit/consume/revalidate;
- ProcessingRequirement interactions;
- Go/server request allow_external + expected_recipient safety fields.

Delete:
- saved connection allow_external/external_recipients authority-shaped fields;
- Flutter allowExternal/externalRecipients helper state;
- old ModelTransport / Runner branch.

The server request field is not the old product consent toggle.

## 2. Migrate legacy runner callers

Current caller audit:

- app example local_model_smoke -> FoundationModelRunner.
- provider server-source tests -> ServerModelRunner.
- runner implementations/exports.

Repeat search before editing.

Migrate:
- full owner smoke claims to InferenceService + canonical provider;
- adapter-specific tests to FoundationModelProvider / ServerModelProvider / PreparedModelTransport.

Do not keep public runners “for tests”.

Delete:
- crates/modules/inference/src/ports/model_transport.rs;
- ModelTransport / ModelTransportRequest / ModelTransportResponse exports;
- the old inference-local ModelStep duplicate;
- FoundationModelRunner;
- ServerModelRunner;
- new_model_only and runner-only conversion helpers.

Keep the canonical floe_agent_contract::ModelStep.

## 3. Delete legacy route types

Current route_config mixes legacy and canonical values.

Delete after moving legitimate current values:
- ModelRouteConfig;
- RemoteRoute;
- RoutePairing;
- LEGACY_INFERENCE_CONSUMER.

Retain/move as needed:
- admitted saved connection transport identity;
- server purpose/profile availability;
- external recipient syntax validation.

Do not leave route_config as a forwarding compatibility shell.

## 4. Remove saved global consent

Rust SavedServerConnection becomes:

~~~text
base_url
token
client_id
person_id
device_id
~~~

Remove consent consistency validation and Debug fields.

Remove the same fields from the admitted RemoteModelConnection or its replacement.

Keychain decoder:
- current shape no longer requires or stores obsolete consent fields;
- no v2 + legacy decoder;
- no migration from old recipient list into contextual Access consent;
- no automatic keychain deletion on mismatch.

Old development credentials may explicitly require re-pair/reset.

Flutter ServerConnection:
- remove allowExternal;
- remove externalRecipients;
- remove coversExternalRecipient;
- remove withExternalConsent;
- remove JSON allow_external/external_recipients;
- update persistence/pairing tests.

## 5. Request-scoped admitted transport target

Current canonical order is:

~~~text
admit_model_dispatch
 -> budget begin
 -> consume_model_dispatch
 -> PreparedModelTransport.generate
 -> revalidate_model_dispatch
~~~

After saved consent deletion, transport permission must derive from the consumed Access fence.

Introduce or expose a non-forgeable, non-secret target produced only after consume, conceptually:

~~~text
AdmittedDispatchTarget
  DeviceOrServerLocal
  External { exact_recipient }
~~~

Exact type name may differ.

Requirements:
- cannot be constructed from Flutter/wire;
- binds the exact selected candidate/recipient;
- carries no credential or source data;
- post-response fence remains separately revalidated.

Pass it through the canonical provider boundary, e.g.:

~~~text
PreparedModelTransport.generate(CanonicalModelRequest, AdmittedDispatchTarget)
~~~

or an equivalent non-bypassable shape.

Do not infer approval from ModelProfile alone.

Server mapping:
- External exact recipient -> allow_external true + same expected_recipient.
- Device/server-local -> allow_external false + no expected recipient.
- candidate/target mismatch -> integrity failure before network handoff.

Foundation/device transport accepts only a local target.

## 6. Required scenarios

M01 no contextual consent:
- typed NeedsUserAction;
- zero prepared transport/network calls.

M02 exact consent:
- server body allow_external true;
- expected_recipient exact;
- no saved consent state.

M03 wrong recipient:
- consent A cannot authorize B.

M04 revoke before consume/handoff:
- zero network call.

M05 revoke after handoff before release:
- response suppressed;
- dispatched usage retained.

M06 server-local/device:
- allow_external false;
- no external recipient.

M07 saved credential:
- pairing identity can admit;
- cannot authorize external route without contextual consent.

M08 old saved shape:
- never migrates recipients into Access;
- never auto-deletes credential.

M09 prepared-profile/consumed-target mismatch:
- fail before network.

## 7. Residual searches

Legacy branch:

~~~sh
rg -n 'ModelTransport|ModelTransportRequest|ModelTransportResponse|FoundationModelRunner|ServerModelRunner|ModelRouteConfig|RemoteRoute|RoutePairing|LEGACY_INFERENCE_CONSUMER|new_model_only' crates apps
~~~

Expected no legacy production matches. PreparedModelTransport intentionally remains.

Saved product consent:

~~~sh
rg -n 'allowExternal|externalRecipients|withExternalConsent|coversExternalRecipient|external_recipients' apps/client crates
~~~

Expected no current product/saved consent state.

Transport fence:

~~~sh
rg -n 'allow_external|expected_recipient' crates server
~~~

Every remaining match must be request-scoped provider/server transport, server synthetic management, or a deliberate negative fixture.

## 8. Gate

Focused Inference, Access dispatch, provider root/server/foundation, saved connection, Flutter local-server and relevant FFI/protocol tests.

Then:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check

cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

Run Go gates only if server code changes.

Commit only after M01–M09 are direct regression evidence.
