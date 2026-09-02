# Turso Storage Architecture

> Status: Recommended baseline, sync topology requires PoC

## Decision

Plain SQLite를 기본 persistence engine으로 두지 않고 **Turso Database**를 Floe의 local/database foundation으로 사용한다.

Turso는 SQLite-compatible file database 모델을 유지하면서 local-first, sync, CDC, vector search, concurrent-write 등의 확장 방향을 제공한다.

---

# Device Storage

```text
Flutter
   ↓
Rust Floe Core
   ↓
Turso Database
   ↓
local file
```

Flutter가 직접 database SDK를 소유하지 않는다.

DB lifecycle, migration, encryption integration, local query는 Rust Core에서 관리한다.

## 이유

- UI framework와 persistence engine 분리
- macOS/iOS/Android/Windows에서 동일한 core semantics
- offline-first
- local inference/retrieval과 가까운 위치
- Dart ↔ DB per-row bridge 방지

---

# Person / Vault Isolation

Floe의 Account ≠ Person 모델과 Turso의 file/many-database 구조를 결합할 수 있다.

후보:

```text
Instance
├─ system.db
├─ person-a.db
├─ person-b.db
└─ person-c.db
```

또는 민감도에 따라:

```text
Person A
├─ timeline.db
├─ memory.db
└─ sensitive-vault.db
```

그러나 database 수를 지나치게 쪼개는 것은 transaction/query 복잡도를 높일 수 있다.

초기 PoC에서는 **Person당 하나의 logical database**를 우선 검토한다.

---

# Hosted Floe

후보:

```text
Floe Cloud
   ↓
Go Control Plane
   ↓
Turso Cloud
      ├─ Person A DB
      ├─ Person B DB
      └─ ...
```

장점 후보:

- tenant isolation boundary가 명확
- backup/delete/export 단위가 Person과 가까움
- per-person memory database를 만들기 쉬움

실제 pricing/limits/operational model은 Hosted 설계 시점에 다시 검증한다.

---

# Self-hosted Floe

Self-host는 Turso Cloud를 필수 dependency로 만들지 않는다.

후보:

```text
Floe Server
   ↓
Self-hosted Turso Database
```

Turso local sync server 또는 Floe-owned sync service 위에 Turso engine을 사용하는 구성을 검토한다.

---

# Sync Is Not Authorization

Turso의 push/pull replication이 유용하더라도 다음은 Floe가 책임져야 한다.

- 어떤 Account가 어떤 Person에 접근 가능한가
- 어떤 Device가 살아 있는가
- 어떤 데이터 class를 device에 sync할 것인가
- 삭제/forget을 어떻게 전파할 것인가
- key rotation/revocation
- connector-derived provenance

즉:

```text
Turso = storage / replication primitive

Floe = identity / policy / semantics
```

으로 본다.

---

# Memory Search

Turso의 native vector/search 기능은 Personal Memory에 잠재적으로 매우 잘 맞는다.

그러나 Memory abstraction을 Turso vector syntax에 직접 결합하지 않는다.

```text
Memory Retrieval Interface
       ↓
Turso-backed implementation
```

으로 둔다.

향후 다른 local index나 specialized search를 추가할 수 있어야 한다.

---

# Migration

Turso가 SQLite-compatible하더라도 version compatibility와 현재 구현되지 않은/새로운 기능을 명시적으로 관리한다.

Floe schema migration은 Floe가 소유한다.

---

# PoC Checklist

- Rust embedded Turso: macOS
- Rust embedded Turso: iOS
- Rust embedded Turso: Android
- Rust embedded Turso: Windows
- encrypted local file
- schema migration
- 2-device push/pull
- conflicting writes
- delete propagation
- 100k+ Timeline/Memory rows
- vector retrieval latency
- DB file per Person operational cost
