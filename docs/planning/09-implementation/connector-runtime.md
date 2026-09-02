# Connector Runtime

> Status: Recommended baseline

## Problem

Activepieces의 주요 가치 중 하나는 TypeScript 기반의 방대한 connector 구현이다.

이를 얻기 위해 Floe Server 전체를 TypeScript로 만들 필요는 없다.

## Recommended Separation

```text
Floe Server (Rust)
      │
      │ typed RPC
      ▼
Connector Host (Node.js / TypeScript)
      │
      ├─ Activepieces-derived adapters
      ├─ Floe TS connectors
      └─ OAuth/API libraries
```

## Why Node.js Here

Connector는 대부분:

- OAuth
- HTTP API
- pagination
- webhook
- JSON transformation

과 같은 I/O-bound workload다.

이 영역에서 Node overhead보다 upstream compatibility와 maintenance cost가 훨씬 중요하다.

Node LTS를 기본 runtime으로 두는 것은 Activepieces/JS package ecosystem과의 호환성을 높인다.

## Do Not Run Workflow Engine

Connector Host에는 다음이 필요하지 않다.

- visual workflow
- arbitrary graph execution
- generic automation scheduling

필요한 것은:

```text
authenticate
bootstrap
poll/changes
subscribe
execute
```

이다.

## Adapter Layer

Activepieces Piece의 action/trigger를 직접 Floe Core semantics로 노출하지 않는다.

```text
Activepieces Piece
     ↓
Adapter
     ↓
Floe Connector Contract
```

## Performance

Connector Host는 Floe의 latency-critical UI/render path가 아니다.

성능이 중요한 connector가 실제로 나타날 경우 해당 connector만 Rust/native implementation으로 교체할 수 있게 interface를 유지한다.

## Isolation

장기적으로 connector를 별도 process/container에서 실행하는 것은 보안상 이점이 있다.

- third-party dependency isolation
- credential scope
- crash isolation
- resource limit

초기 self-host에서는 하나의 Connector Host process로 시작할 수 있다.
