# S5.5 Work and Life Logistics Foundation

> Date: 2026-09-10  
> Acceptance status: bounded Views, isolated Expert contracts and two read-only adapter foundations;
> product transport and live evidence pending

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

## Automated evidence

```sh
cargo test -p floe-agent --test portfolio_context --test portfolio_experts
go -C server test -race ./internal/connectors/github
go -C server vet ./internal/connectors/github
go -C server test -race ./internal/connectors/homeassistant
go -C server vet ./internal/connectors/homeassistant
go -C server test -race ./internal/console
go -C server vet ./internal/console
cargo test -p floe-agent --test connected_context
```

The focused View/privacy and Expert contract tests pass, including both cross-language adapter
fixtures.

## Remaining gate

GitHub and Home Assistant are not yet credential-managed, installed by server startup or consumed by
the client Agent, and no live provider evidence was used. No Slack/Teams, file, travel or delivery
adapter produces these Views yet. Cross-source scenarios and live evidence remain required. S5.5-C4/C5
and S5.5-E7/E8 remain pending.
