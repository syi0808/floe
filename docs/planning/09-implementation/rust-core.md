# Rust Core

> Status: Recommended baseline

## Why Rust

Floe에는 다음 특성이 있다.

- 장기간 실행되는 desktop agent
- encryption / secure storage
- sync engine
- local model runtime
- high-throughput audio/data pipeline
- cross-platform server
- self-host distribution
- strict action validation

이 영역은 Rust와 잘 맞는다.

## Rust Core가 소유할 후보

```text
floe-core
├─ domain primitives
├─ event / mutation model
├─ sync engine
├─ policy/action validation
├─ provenance graph primitives
├─ encryption primitives
├─ local persistence abstraction
├─ memory indexing primitives
└─ model/runtime interfaces where appropriate
```

## Rust Core가 소유하지 않을 것

- Flutter presentation state
- platform UI lifecycle
- HealthKit API itself
- Android Activity/Service lifecycle
- Activepieces TypeScript implementation
- product-specific visual layout

## FFI Strategy

Flutter 공식 `dart:ffi` 경로를 기본 low-level bridge로 본다.

목표:

- generated C ABI
- opaque handles
- batch records
- explicit ownership
- async operation은 callback/event queue 또는 request handle 방식

초기 prototype에서는 third-party bridge generator를 사용할 수 있지만 production core contract는 특정 bridge generator에 종속되지 않게 한다.

## Swift / Kotlin Binding

Rust library를 Swift/Kotlin native module에서도 직접 사용해야 하는 경우 UniFFI를 고려한다.

UniFFI는 Swift/Kotlin binding 생성을 지원하므로:

```text
Rust Core
├─ C/Dart FFI → Flutter
├─ UniFFI → Swift
└─ UniFFI → Kotlin
```

구조를 만들 수 있다.

단, audio sample처럼 hot path에는 high-level binding serialization을 피하고 더 직접적인 buffer interface를 사용한다.

---

# Hot Path Rules

## Never cross language boundary per sample

금지에 가까운 패턴:

```text
audio sample
→ Swift
→ Dart
→ Rust
```

대신:

```text
native audio callback
→ native/Rust ring buffer
→ VAD/wake/STT pipeline
→ semantic event only
→ UI
```

## Large Data

대용량 transcript/audio/embedding buffer는:

- shared file
- memory mapped buffer
- native buffer
- batch transfer

중 적합한 방법을 사용하고 JSON serialization을 hot path에 쓰지 않는다.

---

# Async Runtime

Desktop/server에서는 Tokio를 기본 async runtime 후보로 둔다.

Mobile embedded library에서는 OS lifecycle과 thread ownership을 존중하고 영구 background runtime을 가정하지 않는다.

## Error Boundary

FFI를 넘는 Rust panic은 허용하지 않는다.

- panic containment
- typed error
- stable error code
- diagnostic metadata

를 사용한다.
