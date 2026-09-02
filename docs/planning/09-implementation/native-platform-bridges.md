# Native Platform Bridges

> Status: Recommended baseline

## Principle

OS가 lifecycle/security/permission을 지배하는 기능은 native language로 작성한다.

Flutter Platform Channel은 control-plane에서 사용할 수 있으나 high-frequency data path에는 사용하지 않는다.

---

# Apple

## Swift Responsibilities

후보:

- HealthKit
- EventKit
- Contacts
- App Intents
- notification categories/actions
- Keychain access
- audio/session integration where Swift API is simplest
- extension targets

## macOS

저수준 desktop 기능은 기능별로 Swift와 Rust를 비교한다.

Rust의 `objc2`/system crate 또는 Swift helper 중 유지보수성이 높은 쪽을 선택한다.

UI framework를 이유로 native capability를 Dart로 재구현하지 않는다.

---

# Android

## Kotlin Responsibilities

후보:

- Health Connect
- Assistant role / VoiceInteractionService
- foreground service
- activity/service lifecycle
- notification action
- Android Keystore
- WorkManager
- app widgets/system surfaces

## Rust

CPU-heavy/local shared logic은 NDK library로 Rust Core에 유지한다.

---

# Windows

Windows에서는 Rust `windows-rs`를 우선 검토한다.

후보:

- startup/background agent
- tray/global shortcut
- Windows Hello bridge
- audio/system event
- native notification
- process/window context

COM/WinRT에서 C++가 현저히 단순한 특정 API가 있다면 얇은 C++ adapter를 허용한다.

---

# Bridge Types

## Platform Channel

적합:

- permission request
- open settings
- one-shot OS command
- lifecycle event

부적합:

- audio streaming
- large health dataset
- high-frequency sensor samples
- frame-sensitive rendering data

## FFI

적합:

- Rust Core
- crypto
- local DB
- local inference
- batch transformations
- hot compute

## Local IPC

Desktop UI ↔ resident Device Agent.

exact transport는 PoC로 결정한다.

후보:

- Unix domain socket / named pipe
- protobuf RPC
- authenticated loopback transport

성능보다 lifecycle/security/cross-language 구현 난이도를 함께 평가한다.
