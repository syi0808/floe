# S5.5 Work and Life Logistics Foundation

> Date: 2026-09-11
> Acceptance status: managed read-only adapter and Agent paths; live evidence and cohort breadth
> pending

## Delivered boundary

- Added a selected-scope Work Context View for bounded file/project/meeting-decision/communication
  excerpts, status, blocker and next-action evidence. It carries an opaque workspace scope handle and
  has no absolute paths, organization-wide search or full-document projection.
- Added a Life Logistics View for bounded reservation/travel/delivery/errand/home-state summaries,
  status, timing and attention need. It carries no payment, access code, unlock instruction or raw
  webhook payload.
- Added isolated Work Context and Life Logistics Expert roles. Work results link blockers and next
  actions to supplied evidence and preserve runtime-owned workspace scope. Logistics results link
  preparation recommendations and urgency to evidence and explicitly mark approval expectation.
- Neither Expert receives capabilities. Unknown output fields, invented source handles, oversized
  content, scope escape and attempted execution fields fail closed.
- Added a server-native, read-only GitHub Issues adapter for one explicitly configured repository.
  It uses GET-only REST, direct networking, no redirects, TLS except loopback tests, bounded response
  and issue count, ignores pull requests and hashes repository/issue identity in exported handles.
  Only bounded title/body excerpt/status/blocker metadata enters Work Context.
- The GitHub connector publishes a common server execution descriptor with one Observe capability
  and no Act authority. Static View and snapshot fixtures cross the Go/Rust strict validators.
- Added a server-native, read-only Home Assistant adapter for an explicit allowlist of at most 16
  sensor, binary-sensor, climate, light or switch entities. It rejects security-control domains,
  redirects, non-TLS remote endpoints, endpoint subpaths and duplicate entities. Only friendly name,
  state and opaque evidence handles enter Life Logistics; native entity IDs and attributes do not.
- The Home Assistant connector publishes one Observe capability and no Act authority. Static Life
  Logistics and snapshot fixtures cross the same Go/Rust strict validators.
- Added configured-scope service boundaries and paired, authenticated View routes for Work Context
  and Life Logistics. Requests contain only the schema version: repository and entity selections
  remain server-owned, unknown fields fail closed, and both snapshots join the common connection
  inventory when their runtimes are installed.
- Paired general conversations now advertise read-only Work Context and Life Logistics capabilities
  and stateless A2A Expert cards. Delegation fetches and strictly validates a fresh bounded View,
  then runs the matching isolated Expert without granting it provider actions.
- The loopback management console now configures one selected GitHub repository and one bounded
  Home Assistant entity allowlist. Tokens live only in macOS Keychain, connector selection persists
  without credentials in private server state, startup restores runtimes, and disconnect deletes
  the credential. The dashboard exposes these setup and disconnect controls.
- GitHub and Home Assistant services publish typed revoked, rate-limited, partial-fetch and
  unavailable connection failures. A fresh prior View remains visible only as degraded and only
  until its declared expiry; provider failure no longer drops the whole connection inventory.
- Added a server-native Slack adapter for one explicitly selected channel or thread. It performs a
  single GET-only history/replies read, exports bounded message text as Work Context communication
  evidence, hashes channel/message identity, and excludes files, reactions, profiles and provider
  IDs. Invalid auth, missing scope and rate limits become typed connector failures.
- Slack configuration and tokens use the same private console/Keychain lifecycle. GitHub and Slack
  Work Context Views merge behind one provider-neutral Agent route with deterministic aggregate
  handles, duplicate rejection and partial-provider tolerance.

## Automated evidence

```sh
cargo test -p floe-agent --test portfolio_context --test portfolio_experts
go -C server test -race ./internal/connectors/github
go -C server vet ./internal/connectors/github
go -C server test -race ./internal/connectors/homeassistant
go -C server vet ./internal/connectors/homeassistant
go -C server test -race ./internal/console
go -C server vet ./internal/console
go -C server test -race ./internal/connectors/slack
go -C server vet ./internal/connectors/slack
cargo test -p floe-agent --test connected_context
cargo test -p floe-ffi
cargo test --workspace
node --check server/internal/console/web/app.js
```

The focused View/privacy and Expert contract tests pass, including both cross-language adapter
fixtures. A paired synthetic HTTP integration test proves both Work Context and Life Logistics
delegations fetch fresh Views and return source-linked typed A2A artifacts.

## Remaining gate

No live provider evidence was used. The Agent path is stateless rather than a durable registry
assignment. No Teams, file, travel or delivery adapter produces these Views yet. Cross-source
scenarios and live evidence remain required. S5.5-C4/C5 and S5.5-E7/E8 remain pending.
