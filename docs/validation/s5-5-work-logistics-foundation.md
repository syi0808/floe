# S5.5 Work and Life Logistics Foundation

> Date: 2026-09-10  
> Acceptance status: bounded Views and isolated Expert contracts; live adapters pending

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

## Automated evidence

```sh
cargo test -p floe-agent --test portfolio_context --test portfolio_experts
go -C server test -race ./internal/connectors/github
go -C server vet ./internal/connectors/github
cargo test -p floe-agent --test connected_context
```

The focused View/privacy and Expert contract tests pass 4/4.

## Remaining gate

GitHub is not yet credential-managed, scheduled or exposed to a product conversation, and no live
repository evidence was used. No Slack/Teams, file, travel, delivery or Home Assistant adapter
produces these Views yet. Cross-source scenarios and live evidence remain required. S5.5-C4/C5 and
S5.5-E7/E8 remain pending.
