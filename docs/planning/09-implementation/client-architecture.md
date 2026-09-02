# Client Architecture

> Status: Recommended baseline

## Goal

Main UI 공유율은 높이되, 성능과 native integration을 위해 Flutter process에 모든 책임을 넣지 않는다.

## Logical Layers

```text
┌──────────────────────────────────────┐
│             Flutter App              │
│                                      │
│ Day Canvas                           │
│ Capture UI                           │
│ Settings                             │
│ Memory Inspection                    │
│ Connector UI                         │
└──────────────────┬───────────────────┘
                   │
        snapshots / commands
                   │
                   ▼
┌──────────────────────────────────────┐
│              Floe Core               │
│                Rust                  │
│                                      │
│ Domain                               │
│ Sync                                 │
│ Memory primitives                    │
│ Policy                               │
│ Crypto                               │
│ Local persistence orchestration      │
└──────────────┬──────────────┬────────┘
               │              │
               ▼              ▼
        Native Adapter    Model Runtime
```

---

# Mobile Process Model

iOS/Android에서는 별도 permanent daemon을 기본 전제로 하지 않는다.

```text
Flutter App Process
├─ Dart UI
├─ embedded Rust Core
└─ Native Plugins
   ├─ Swift
   └─ Kotlin
```

Rust Core는 static/shared native library로 application에 embed한다.

Flutter ↔ Rust에는 가능하면 direct FFI를 사용한다.

Native OS API는 Flutter Plugin 또는 native wrapper를 사용한다.

---

# Desktop Process Model

macOS/Windows에서는 ambient voice와 background context 때문에 UI와 resident process를 분리하는 것이 좋다.

```text
Floe Device Agent
    Rust/native resident process
        │
        ├─ wake word
        ├─ audio pipeline
        ├─ local models
        ├─ secure store
        ├─ sync
        └─ OS context
        │
        ▼
Local IPC
        │
        ▼
Flutter Floe UI
```

## 이유

- UI가 종료되어도 ambient 기능 유지 가능
- Flutter engine을 background에서 계속 유지할 필요가 없음
- voice/audio hot path가 UI isolate와 분리됨
- crash boundary 분리
- updater/service lifecycle을 별도로 관리 가능

---

# UI State Boundary

Flutter에 full domain engine을 중복 구현하지 않는다.

Flutter가 가지는 것은 주로:

- presentation state
- optimistic UI state
- transient interaction state
- immutable/render-friendly snapshots

이다.

Canonical domain mutation은 Floe Core를 통해 수행한다.

---

# Avoid Chatty FFI

다음 패턴을 피한다.

```text
every widget
  ↓
FFI query
  ↓
Rust
```

또는:

```text
every audio frame
  ↓
Platform Channel
  ↓
Dart
  ↓
FFI
```

대신:

```text
Rust/Core
  ↓
batched immutable snapshot
  ↓
Flutter render
```

와 같이 coarse-grained boundary를 둔다.

---

# Suggested Client Data Flow

```text
OS Event
  ↓
Native Adapter / Device Agent
  ↓
Rust Core
  ↓
Domain state update
  ↓
Snapshot/Event
  ↓
Flutter UI
```

사용자 action:

```text
Flutter UI
  ↓
Typed Command
  ↓
Rust Core
  ↓
Policy / validation
  ↓
Native or Connector executor
```

---

# System Extensions

다음은 Flutter UI process와 별개 target이 될 수 있다.

- iOS App Intent extension
- widgets
- Android Assistant/VoiceInteraction service
- Android foreground/background service
- macOS login item/helper
- Windows startup service/tray helper

가능한 한 Core protocol과 domain type을 공유하되 lifecycle은 OS 규칙을 따른다.
