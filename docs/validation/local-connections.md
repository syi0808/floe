# Local connection console validation

Date: 2026-09-05, macOS arm64. Extends S2, not S4 acceptance.

## Automated evidence

- Rust workspace: 34 tests; Clippy warnings denied. New tests reject remote,
  credential-bearing, path/query/fragment and invalid-port gateway addresses and
  malformed bearer credentials before networking.
- Flutter: 56 tests, analyzer clean; Settings placement, local address normalization, credential
  persistence, invalid-store recovery, redirect refusal and panel disposal covered.
- Go: management/inference separation, exact Host/Origin/CSRF, cookie attributes,
  pairing approval/expiry/proof bounds, restart persistence and revocation, credential
  redaction/failure atomicity, endpoint key isolation, consent and synthetic tests.
- Go OAuth fixtures: PKCE/state callback, token exchange, refresh rotation, Keychain
  boundary, logout, no-tool structured Codex request and bounded SSE output.
- Opt-in Security.framework test: disposable synthetic credential write/read/delete
  passes. No real provider API key is read or registered by this test.
- Actual Flutter → Rust → Go console → synthetic provider integration passes on
  a dynamically selected non-default port, using a paired credential rather than
  an environment token. External consent blocks the provider first; a valid proposal
  then passes domain validation; client revocation is observed. No Calendar mutation.
- Original headless Flutter/Rust/Go fixture still passes separately.
- macOS debug app builds and strict signature verification passes.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd server
FLOE_TEST_KEYCHAIN=1 go test -race ./...
go vet ./...
cd ../apps/client
flutter test
flutter analyze
flutter test integration/local_server_pairing_test.dart
FLOE_INFERENCE_TOKEN="$(openssl rand -hex 24)" flutter test integration/focus_inference_test.dart
```

The last command needs port 8431 free. Stop the interactive server first, or run
that legacy fixture before starting it. The pairing suite uses its own port and
temporary server state; it does not touch the live node or native app Keychain.

## Interactive checkpoint

- Local node started at `http://127.0.0.1:8431`; dashboard opens and login works.
- Browser dashboard supports direct Codex OAuth and reports whether inference is enabled.
- The operator confirmed native app code entry, dashboard approval and pairing work.
  Native UI automation still times out; Keychain persistence across an app relaunch
  and independent visual verification remain manual checkpoints.
- No real provider is registered and no live OAuth consent or inference is claimed.

## Manual walkthrough

1. Start the node using the [server README](../../server/README.md), unlock the
   dashboard, enter its address in **Settings → Remote server** and compare the
   pairing code.
2. Approve; verify `Connected to Floe server`. Restart Floe and the server;
   check again without a shell token. Revoke in the dashboard and verify denial.
3. Add a model; test with synthetic data. Confirm an external test requires explicit
   confirmation, then choose a default target in Floe. No automatic model download.
4. Check blank-key retention, endpoint-change key isolation, replacement and deletion.
   Deny/lock Keychain access and verify a recoverable error, without secret display.
5. Start Codex browser login, complete consent yourself, verify completion, cancel a
   new pending flow, disconnect and reconnect. Check five-minute expiry with the page
   closed. Verify this does not alter an existing non-Floe Codex login.
6. Add a Codex OAuth target, run the synthetic test, then request a focus proposal.
   Verify no tool request, minimal context, structured output, cancellation, token
   refresh and subscription-limit behavior before treating the adapter as live-verified.

Real model quality, S1 Calendar gates and S2 acceptance remain pending.
