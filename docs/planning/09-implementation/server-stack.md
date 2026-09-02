# Server Stack

> Status: Recommended baseline

## Goals

- self-host가 어렵지 않을 것
- Hosted Floe로 자연스럽게 확장 가능할 것
- multi-user / multi-device control plane에 적합할 것
- connector/model provider의 많은 I/O를 효율적으로 처리할 것
- 운영 dependency를 불필요하게 늘리지 않을 것

---

# Recommended Stack

```text
Server / API Control Plane
    Go

Database
    Turso Database / Turso Cloud

Device Local Database
    Embedded Turso through Rust Core


Object Storage
    S3-compatible only when required

Queue / Cache
    initially avoid mandatory Redis/NATS/Kafka
```

---

# Why Go

Floe Server는 주로 다음 작업을 수행한다.

```text
HTTP / RPC
Device Sessions
Authentication
Authorization
Connector Coordination
Turso Access
Event Routing
Model Provider Calls
Background Jobs
Serialization
```

대부분 network/I/O-bound control-plane workload다.

Go를 선택하는 이유:

- goroutine 기반 concurrency가 server model과 잘 맞음
- HTTP/network standard library가 강함
- static deployment가 단순함
- build/test iteration이 빠름
- pprof 등 production profiling이 좋음
- self-host binary 운영 부담이 낮음
- Rust보다 application service 코드를 변경하기 쉬움

Rust가 server보다 절대적으로 느리거나 부적합해서가 아니다.

Rust는 no-GC와 매우 낮은 overhead로 더 높은 최대 성능을 만들 수 있지만 Floe Server에서 그 차이가 주요 제품 병목일 가능성은 낮다.

실제 latency는 대체로:

- database
- external connector
- model inference
- network

가 더 크게 지배할 가능성이 높다.

---

# Why Not Share the Whole Rust Core with Server

Client와 server가 같은 언어이면 domain code reuse가 쉬운 장점이 있다.

하지만 이를 위해 Server 전체를 Rust로 고정하지 않는다.

대신 공유해야 할 것은 우선:

- protocol schema
- identifiers
- action schema
- validation test vectors
- migration/schema definitions

이다.

Server authority는 Go가 소유하고 Device Rust Core는 local validation/UX를 위한 conservative mirror를 가질 수 있다.

정확히 동일해야 하는 security primitive가 실제로 나타나면 작은 Rust library/portable artifact로 분리하는 방식을 별도로 검토한다.

---

# Turso as the Database Foundation

Floe에서는 plain SQLite 대신 Turso Database를 기본 database engine으로 둔다.

## Device

```text
Flutter UI
   ↓
Rust Floe Core
   ↓
Embedded Turso
```

Device의 canonical local working set은 network 없이 읽고 쓸 수 있어야 한다.

## Self-host Server

후보:

```text
Go Floe Server
    ↓
local/self-hosted Turso
```

Instance 규모와 isolation 요구에 따라:

- instance database
- Person별 database
- vault별 database

중 적합한 database topology를 결정한다.

## Hosted Floe

Turso Cloud를 우선 hosted database 후보로 둔다.

특히 Person 또는 vault별로 database를 분리할 수 있는 many-database architecture는 Floe의 privacy/isolation model과 궁합이 좋을 수 있다.

---

# Turso Sync

Turso Sync는 Floe에 매우 유망하다.

```text
Device Turso
    ↕ push / pull
Server / Cloud Turso
```

하지만 DB replication과 Floe application authorization을 동일시하지 않는다.

PoC에서 다음을 확인한다.

- multi-device conflict semantics
- selective data sync
- encryption compatibility
- Person별 token/authorization
- device revoke
- deletion propagation
- self-host authentication
- schema migration
- offline bootstrap

필요하면:

```text
Floe Sync Protocol
      ↓
Turso Database
```

형태를 유지하고 Turso Sync는 내부 최적화로만 사용한다.

---

# Avoid Mandatory Distributed Infra Early

MVP부터 다음을 모두 요구하지 않는다.

```text
Redis
Kafka
NATS
Elasticsearch
separate Vector DB
```

Go server + Turso를 최소 baseline으로 한다.

background work는 초기에는:

- durable job table
- DB-backed outbox
- in-process Go workers

로 시작할 수 있다.

---

# Process Layout

Self-host baseline:

```text
floe-server
    Go

turso
    embedded / local service depending topology
```

선택 사항:

```text
object storage
local inference service
reverse proxy
```

목표는 여전히:

```bash
docker compose up -d
```

수준의 setup이다.

## Native Server Connectors

SaaS connectors that require server-side background/webhook execution are implemented as Go packages/modules inside the server codebase by default.

```text
floe-server
├─ connectors/
│  ├─ gmail
│  ├─ googlecalendar
│  └─ ...
```

Portable ConnectorSpec connectors share a generic Go execution engine.

If third-party connector isolation becomes necessary, native connector workers can later be moved into separate processes without requiring a language runtime such as Node.
