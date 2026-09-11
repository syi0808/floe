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
- The paired Floe client configures one selected GitHub repository and one bounded Home Assistant
  entity allowlist. Person-scoped tokens live only in macOS Keychain, connector selection persists
  without credentials in private server state, startup restores runtimes, and disconnect deletes
  the credential. The management dashboard exposes no personal connector controls or state.
- GitHub and Home Assistant services publish typed revoked, rate-limited, partial-fetch and
  unavailable connection failures. A fresh prior View remains visible only as degraded and only
  until its declared expiry; provider failure no longer drops the whole connection inventory.
- Added a server-native Slack adapter for one explicitly selected channel or thread. It performs a
  single GET-only history/replies read, exports bounded message text as Work Context communication
  evidence, hashes channel/message identity, and excludes files, reactions, profiles and provider
  IDs. Invalid auth, missing scope and rate limits become typed connector failures.
- Slack configuration and tokens use the same paired-client/Keychain lifecycle. GitHub and Slack
  Work Context Views merge behind one provider-neutral Agent route with deterministic aggregate
  handles, duplicate rejection and partial-provider tolerance.
- Added a server-native Microsoft Teams adapter for one explicitly selected team/channel. It makes
  one GET-only Microsoft Graph request for at most 50 root messages under the delegated
  `ChannelMessage.Read.All` scope and emits the same bounded Work Context communication shape.
  Message markup is reduced to plain untrusted text; user identity, attachments, provider IDs,
  URLs, reactions and send authority remain outside the View. This checkpoint includes the strict
  adapter/service and cross-language fixtures.
- Microsoft Teams now has an isolated PKCE OAuth credential with the exact delegated
  `ChannelMessage.Read.All` scope, private team/channel selection persistence, startup restoration
  and dashboard lifecycle controls. Configured Teams evidence joins GitHub, Slack and Drive behind
  the same provider-neutral Work Context merge; credentials and selected provider IDs remain
  outside Agent requests and exported Views.
- Added a Google Drive adapter for one explicitly selected folder. Each foreground read lists at
  most eight recent entries and reads only supported text or Google Document content, capped at
  16 KiB fetched and 2 KiB projected per file. Binary/unsupported types, file/folder IDs, MIME
  details and provider URLs stay outside Work Context; content remains ephemeral.
- Drive uses a separate PKCE OAuth credential and exact Drive read-only scope, isolated from the
  Gmail OAuth bundle. The dashboard manages browser login and folder selection independently, and
  Drive joins the same multi-provider Work Context route and typed failure lifecycle.
- Gmail metadata now projects explicit reservation, travel, delivery and errand phrases into
  bounded Logistics candidates. Classification never asserts that an event occurred: each item is
  marked `mail_candidate`, carries only subject-level summary and opaque evidence, and has no parsed
  payment, access code or action authority.
- Gmail candidates and Home Assistant state merge behind the Life Logistics route with deterministic
  aggregate provenance, duplicate rejection, earliest-expiry enforcement and partial-source
  tolerance. Gmail's connector descriptor now declares this additional Observe-only View.

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
go -C server test -race ./internal/connectors/microsoftteams
go -C server vet ./internal/connectors/microsoftteams
go -C server test -race ./internal/microsoftauth ./internal/console ./cmd/floe-server
go -C server vet ./internal/microsoftauth ./internal/console ./cmd/floe-server
go -C server test -race ./internal/connectors/googledrive ./internal/googleauth
go -C server vet ./internal/connectors/googledrive ./internal/googleauth
go -C server test -race ./internal/connectors/common ./internal/connectors/gmail
go -C server vet ./internal/connectors/common ./internal/connectors/gmail
cargo test -p floe-agent --test connected_context
cargo test -p floe-ffi
cargo test --workspace
node --check server/internal/console/web/app.js
```

The focused View/privacy and Expert contract tests pass, including both cross-language adapter
fixtures. A paired synthetic HTTP integration test proves both Work Context and Life Logistics
delegations fetch fresh Views and return source-linked typed A2A artifacts.

## Implementation increment checkpoint

Implementation increment 7 is connected end to end: bounded Gmail logistics candidates and the
selected Home Assistant adapter merge through the authenticated `life.logistics` route, and the
runtime delegates the fresh strict View to the capability-free Life Logistics Expert. Home
Assistant satisfies the increment's travel/delivery/home adapter choice in code; this statement is
an implementation checkpoint, not live S5.5-C5 acceptance.

## Remaining gate

No live provider evidence was used. The Agent path is stateless rather than a durable registry
assignment. Microsoft Teams still needs a real tenant/admin-consent run, and Home Assistant needs a
real selected-source run to satisfy the live travel/delivery/home gate. Broader cross-domain
scenarios and live evidence are also required. S5.5-C4/C5 and S5.5-E7/E8 remain pending.
