# Repository Layout

> Status: Recommended starting point

## Monorepo First

초기에는 cross-layer refactor가 많을 가능성이 높으므로 monorepo를 권장한다.

개념적 구조:

```text
floe/
├─ apps/
│  ├─ client/                  # Flutter main app
│  ├─ server/                  # Go server / control plane
│
├─ server/
│  ├─ internal/
│  ├─ cmd/
│  └─ go.mod
│
├─ crates/
│  ├─ floe-core/
│  ├─ floe-domain/
│  ├─ floe-memory/
│  ├─ floe-sync/
│  ├─ floe-policy/
│  ├─ floe-crypto/
│  ├─ floe-protocol/
│  └─ floe-device-agent/
│
├─ native/
│  ├─ apple/
│  │  ├─ FloeAppleKit/
│  │  └─ extensions/
│  ├─ android/
│  │  └─ floe-android-platform/
│  └─ windows/
│
├─ connectors/
│  ├─ spec/                    # declarative connector definitions
│  ├─ importers/
│  │  └─ activepieces/         # build-time source importer
│  └─ native/
│     └─ ...
│
├─ models/
│  ├─ manifests/
│  └─ tooling/
│
├─ proto/
│  └─ ...
│
├─ deploy/
│  ├─ docker/
│  └─ compose/
│
└─ docs/
   └─ planning/
```

실제 crate/package 분리는 초기 코드 양에 따라 더 적게 시작해도 된다.

## Avoid Premature Package Explosion

초기에는:

```text
floe-core
floe-server
floe-device-agent
```

정도로 시작하고 domain boundary가 안정된 뒤 crate를 분할하는 것도 가능하다.

문서상의 architecture boundary와 source repository package boundary는 반드시 1:1일 필요가 없다.

## Generated Bindings

FFI/protocol generated code는 명시적 generated directory에 둔다.

source-of-truth는:

- Rust interface/schema
- protobuf schema
- connector manifest

등 하나만 유지한다.

## CI Matrix

장기적으로 최소:

- macOS arm64
- Windows x64/arm64 where feasible
- Android
- iOS

에 대해 shared Rust/Flutter build를 검증한다.

Native platform integration test는 별도 lane으로 둔다.

## Connector Placement

```text
server/internal/connectors/
    Go server-native SaaS connectors

crates/floe-core/
    Rust connector traits/runtime for device-native connectors

connectors/spec/
    portable declarative connector definitions

connectors/importers/
    development/build-time import tools
```

No Node runtime is required by the default repository/runtime topology.
