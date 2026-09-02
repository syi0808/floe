# Integration Fabric

> Status: Core infrastructure

## 중요성

Floe의 핵심 가치 중 하나는 다양한 앱, 서비스, OS로부터 데이터를 안전하고 안정적으로 연결하는 것이다.

이 계층은 Agent보다 장기적으로 더 중요한 기술 자산이 될 수 있다.

## 책임

```text
Authentication
Authorization
Initial Sync
Incremental Sync
Subscriptions
Normalization
Conflict Resolution
Revisions
Deletion
Provenance
Token Management
Capability Discovery
```

## Integration ≠ Automation

Floe는 다음 workflow graph를 핵심 execution model로 사용하지 않는다.

```text
Trigger
→ Node
→ Node
→ Node
```

대신:

```text
External Event
      ↓
Connector
      ↓
Floe Event
      ↓
Domain / Expert
      ↓
Manager
```

## Native vs SaaS

### Native/OS 가까운 source

가능하면 Floe native implementation.

예:

- HealthKit
- Health Connect
- EventKit 계열
- OS Contacts

### SaaS

검증된 connector ecosystem을 적극적으로 활용.

예:

- Gmail
- Google Calendar
- Outlook
- Notion
- GitHub

## Source of Truth

같은 object를 여러 경로로 중복 ingest하지 않도록 source identity와 sync policy가 필요하다.

## Connector UI

연결 onboarding은 모든 권한을 한 번에 묻지 않는다.

가치가 필요한 시점에 progressive connection/permission을 요청하는 방향을 선호한다.

## Runtime Placement

Connector semantics are shared, but execution location is selected by data ownership and lifecycle.

```text
OS / Sensitive / Local
→ Device-native connector
→ Rust + Swift/Kotlin/native API

Always-online SaaS
→ Server-native connector
→ Go

Portable REST/OAuth connector
→ Floe ConnectorSpec
→ Go and/or Rust implementation
```

Flutter is not a connector execution runtime by default. It owns connection and permission UX, while credentials and execution remain in the appropriate secure/native layer.

## No Mandatory JavaScript Runtime

Production Floe does not require Node merely to reuse third-party connector ecosystems.

A JavaScript compatibility host may exist later as an optional extension, but is not part of the baseline self-host or client runtime.
