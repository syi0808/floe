# Technology Selection

> Status: Recommended baseline  
> Goal: 크로스플랫폼 생산성과 네이티브 수준의 성능/OS 통합을 동시에 확보한다.

## Selection Criteria

Floe의 기술 선택 우선순위는 다음과 같다.

1. **macOS / Windows / iOS / Android를 모두 1급으로 지원**
2. UI에서 60/120Hz에 충분한 예측 가능한 렌더링 성능
3. Health, Voice, Background Service, Secure Storage 등 네이티브 API에 깊게 접근 가능
4. local inference / native library와 낮은 오버헤드로 연동 가능
5. 오픈소스 및 self-hosting 철학과 잘 맞을 것
6. 장기 유지보수 시 특정 플랫폼/벤더에 과도하게 종속되지 않을 것
7. 성능이 중요한 코어를 UI 프레임워크 바깥으로 분리할 수 있을 것

---

## Recommended Baseline

```text
Main App UI
    Flutter / Dart

Performance & Shared Core
    Rust

Native OS Adapters
    Swift / Kotlin / Rust(windows-rs, macOS helper where appropriate)

Connector Runtime
    TypeScript / Node.js

Server / Control Plane
    Go

Database Engine / Local Store
    Turso Database

Hosted Database
    Turso Cloud or self-hosted Turso deployment
```

핵심은 "Flutter로 모든 것을 구현"하는 것이 아니다.

```text
Flutter UI Shell
      │
      ▼
Rust Core / Device Agent
      │
      ├─ Native Swift Adapter
      ├─ Native Kotlin Adapter
      └─ OS-specific Rust Adapter
```

---

# Why Flutter for the Main UI

Floe의 Day Canvas는 플랫폼 기본 widget을 그대로 조합하는 UI보다 **제품 고유의 단순하고 차분한 visual language**가 중요하다.

Flutter는 이 요구와 잘 맞는다.

현재 Flutter는 Android, iOS, macOS, Windows를 공식 target으로 제공하고 Dart application code를 native AOT 형태로 배포한다.

Flutter의 자체 rendering pipeline은 platform widget bridge에 매 frame 의존하지 않으며, 현재 Impeller가 모바일뿐 아니라 macOS/Windows에도 확대되어 예측 가능한 rendering 성능을 목표로 한다.

## 장점

- 네 플랫폼의 UI code 공유율을 높일 수 있음
- custom Day Canvas를 일관되게 구현하기 좋음
- native AOT
- 자체 rendering engine
- animation/layout 성능을 직접 통제하기 쉬움
- Dart FFI를 통해 native/Rust library와 직접 연결 가능
- 필요 시 Swift/Kotlin/C++ plugin 작성 가능

## 약점

- OS native widget fidelity가 자동으로 보장되지는 않음
- Flutter engine 자체의 binary/memory 비용이 존재
- App Intent, Android Assistant Service, system extension 등은 결국 native target이 필요
- desktop background helper/daemon 역할까지 Flutter process에 넣는 것은 적절하지 않음

Floe에서는 이 단점 대부분을 "UI Shell과 Device Agent 분리"로 회피한다.

---

# Why Not Compose Multiplatform as Default

Kotlin Multiplatform과 Compose Multiplatform은 매우 강력한 대안이며 Android/iOS/Desktop UI가 현재 stable 범위에 있다.

특히 mobile-first 제품이라면 최우선 후보가 될 수 있다.

그러나 Floe는:

- macOS와 Windows desktop이 매우 중요하고
- ambient voice/background helper가 핵심이며
- shared performance core로 Rust를 활용하고 싶고
- desktop JVM runtime을 기본 배포 구조로 두고 싶지 않다.

따라서 현재 Floe의 기본 UI 선택으로는 Flutter가 더 적합하다고 판단한다.

KMP는 향후 특정 native/mobile module 공유에 부분적으로 사용할 수 있지만 Core architecture의 필수 dependency로 만들지 않는다.

---

# Why Not Tauri as Default

Tauri 2는 Rust core와 system WebView를 이용해 macOS, Windows, iOS, Android를 지원한다.

Desktop utility에는 매우 좋은 선택이다.

하지만 Floe Main UI에는 다음 우려가 있다.

- rendering behavior가 OS WebView implementation에 영향을 받음
- DOM/CSS/JS rendering cost와 browser lifecycle/throttling을 고려해야 함
- mobile deep integration에서는 결국 Swift/Kotlin plugin이 필요
- Day Canvas와 animation을 제품의 핵심 UI로 두는 경우 Flutter보다 rendering consistency가 낮을 수 있음

따라서 Tauri는:

- Admin/utility tool
- 개발용 desktop control panel
- 작은 companion tool

에는 후보가 될 수 있으나 Main Floe Client의 기본 선택으로 두지 않는다.

---

# Principle: Native Where the OS Owns the Experience

다음 영역은 Flutter로 억지 통일하지 않는다.

```text
iOS App Intents / extensions       → Swift
HealthKit                          → Swift
Android Assistant/Health Connect   → Kotlin
Android background service         → Kotlin
Windows system integration         → Rust/windows-rs or native
macOS low-level audio/system API    → Swift/Rust as appropriate
```

Flutter는 **main visual surface**를 담당한다.

OS가 lifecycle과 security model을 소유하는 기능은 native layer가 담당한다.

---

# Decision Summary

| Layer | Recommended |
|---|---|
| Cross-platform UI | Flutter |
| Shared performance core | Rust |
| Apple native API | Swift |
| Android native API | Kotlin |
| Windows low-level integration | Rust + windows-rs |
| Connector ecosystem runtime | TypeScript + Node.js |
| Server / API control plane | Go |
| Database engine | Turso Database |
| Hosted database | Turso Cloud / self-hosted Turso |
| Device local database | embedded Turso via Rust Core |
| Inter-process protocol | versioned binary/structured protocol; exact transport TBD |


---

## Why Go for the Server

Floe Server의 핵심 workload는 다음에 가깝다.

- HTTP / RPC
- device connection
- authentication / authorization
- connector orchestration
- event routing
- Turso access
- model/provider 호출
- serialization
- background jobs

즉 CPU-bound compute engine보다 **networked control plane** 성격이 강하다.

이 영역에서는 Rust의 최대 성능과 no-GC 특성보다 Go의 다음 특성이 더 큰 실용적 장점이 될 수 있다.

- goroutine 기반 concurrency
- 단순한 deployment
- 빠른 build
- 작은 운영 복잡도
- 좋은 HTTP/network standard library
- production debugging/profile tooling
- I/O service를 빠르게 변경하기 쉬운 코드 구조

Rust는 여전히 다음 영역에 집중한다.

- cross-platform Device Core
- local inference
- audio hot path
- crypto primitives
- performance-sensitive sync/local operations
- native/FFI boundary

따라서 언어를 하나로 통일하지 않고 workload에 따라 선택한다.

```text
Performance / Device Data Plane → Rust
Server / Control Plane          → Go
Connector Ecosystem             → TypeScript
UI                              → Flutter/Dart
```

---

## Why Turso Instead of Plain SQLite

Floe는 local-first와 multi-device를 제품의 중심 요구로 가진다.

Turso Database는 SQLite-compatible file database 모델을 유지하면서 다음을 제공하는 방향이 Floe와 잘 맞는다.

- local embedded database
- async architecture
- concurrent-write capability
- change data capture
- vector search
- local/cloud sync
- self-host 가능한 database engine

특히 Device Agent의 Rust Core가 Turso Rust SDK를 직접 소유하면 Flutter/Dart에 DB engine을 결합할 필요가 없다.

```text
Flutter
   ↓
Rust Floe Core
   ↓
Embedded Turso Database
```

Hosted와 self-host에서도 동일 계열의 database semantics를 유지할 가능성이 생긴다.

단, **Turso Sync 자체를 Floe의 authorization/sync protocol로 즉시 확정하지는 않는다.**

Floe에는 다음 요구가 추가로 있기 때문이다.

- Person/Account authorization
- field/data-class encryption
- selective sync
- device revocation
- provenance
- delete/forget semantics
- server policy

따라서 Turso Sync를 하위 replication primitive로 활용할 수 있는지는 별도 PoC로 검증한다.
