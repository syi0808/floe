# Connector Runtime

> Status: Recommended baseline

## Decision

**Node.js / TypeScript runtime is not part of Floe's default runtime.**

Node was previously proposed mainly to execute Activepieces Pieces with minimal modification. Activepieces Pieces are arbitrary TypeScript/npm packages, so drop-in compatibility would make a JavaScript runtime a permanent product dependency.

Floe instead owns a native connector model.

```text
                 Floe Connector Model
                         │
             ┌───────────┴───────────┐
             │                       │
     Device-native Connector   Server-native Connector
             │                       │
          Rust/Core                  Go
       + OS adapters                 │
             │                       │
      HealthKit etc.          Gmail / SaaS etc.
```

Activepieces is primarily treated as a **connector implementation corpus and reference source**, not as a runtime dependency.

---

# Why Not Node by Default

A Node sidecar would provide easy compatibility with TypeScript/npm connectors, but it also introduces:

- another runtime to install and patch
- another long-running process/container
- larger self-host footprint
- additional dependency/security surface
- duplicated lifecycle and logging
- IPC between Go/Rust and Node
- Node/npm-specific connector semantics leaking into Floe

For Floe's self-host and device-agent philosophy, these costs are meaningful.

Therefore:

> **Native first. JavaScript compatibility only as an optional future escape hatch.**

---

# Connector Execution Classes

## 1. Device-native Connector

Used when data/capability belongs to the OS/device or should remain local.

Examples:

```text
HealthKit
Health Connect
EventKit / OS calendars
Contacts
local files
device context
local credential-backed source
```

Architecture:

```text
Native OS API
      ↓
Swift / Kotlin / OS Adapter
      ↓
Rust Floe Core
      ↓
Floe Connector Interface
```

Flutter does not own connector business logic. It provides connection UI, permission explanation, account selection, and status/health UI.

## 2. Server-native Connector

Used for SaaS sources that benefit from always-online execution.

Examples:

```text
Gmail
Google Calendar API
Outlook
Notion
GitHub
Slack
```

Reasons:

- webhook ingress
- token refresh
- scheduled/polling sync
- device-offline operation
- central rate-limit handling

Architecture:

```text
External SaaS
    ↓
Go Connector
    ↓
Floe Server Domain
```

The connector lives directly in the Go server/worker codebase unless isolation becomes necessary.

## 3. Portable Declarative Connector

Many SaaS integrations are structurally repetitive:

```text
OAuth2 / API key
REST request
pagination
webhook
polling
JSON mapping
token refresh
```

For these, Floe can define a declarative `ConnectorSpec`.

Example concept:

```yaml
id: example
auth:
  type: oauth2
  authorization_url: ...
  token_url: ...

actions:
  list_items:
    request:
      method: GET
      path: /v1/items
    pagination:
      type: cursor

triggers:
  item_changed:
    type: webhook
```

The same spec can be consumed by Go server runtime, Rust device runtime when needed, docs/permission UI, and connector capability discovery.

This is more valuable to Floe than sharing one JavaScript implementation.

---

# Activepieces Reuse Strategy

Activepieces Pieces remain valuable because they contain years of integration work:

- authentication shapes
- endpoints
- pagination behavior
- webhook lifecycle
- edge cases
- service-specific payload handling

But Floe does not promise binary/runtime compatibility.

## Tier A — Mechanical Port

Pieces that mostly use standard auth, `httpClient`, standard actions, polling, or webhook helpers can potentially be translated into Floe ConnectorSpec.

A build-time importer can be explored.

```text
Activepieces TypeScript source
       ↓
AST parser / source analyzer
       ↓
supported pattern detection
       ↓
Floe ConnectorSpec
       ↓
Go / Rust runtime
```

Because Floe already uses Rust heavily, an OXC-based TypeScript analyzer is a plausible implementation option.

This importer is a **development/build tool**; Node does not need to ship with Floe.

## Tier B — Assisted Manual Port

Piece contains custom transformations but conventional HTTP/auth behavior.

Use the Piece as reference and port the behavior to Go/Rust.

## Tier C — Custom Native Connector

Piece relies heavily on arbitrary npm packages, provider-specific SDKs, complex streaming/protocol behavior, or JS runtime semantics.

Implement a native Floe connector instead.

---

# Why Full Automatic TypeScript → Rust Translation Is Not a Goal

Activepieces actions and triggers can execute arbitrary TypeScript and import npm packages. Therefore a complete translator would effectively become a JavaScript/Node compatibility layer.

The importer only targets a **well-defined portable subset**. Unsupported Pieces should fail explicitly and fall back to manual/native implementation.

---

# Optional JavaScript Compatibility

A future optional extension may provide:

```text
floe-js-connector-host
```

for users who specifically want unported Activepieces/community Pieces.

It must not be required by Floe Cloud core architecture, standard self-host install, mobile clients, or native connectors.
