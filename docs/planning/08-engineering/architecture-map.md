# Architecture Map

> Status: Working map

## High-level

```text
┌───────────────────────────────────────────┐
│                FLOE CLIENTS               │
│                                           │
│ macOS     Windows      iOS      Android   │
│           Native Device Agents            │
└──────────────────┬────────────────────────┘
                   │
             Device Protocol
                   │
                   ▼
┌───────────────────────────────────────────┐
│                FLOE CORE                  │
│                                           │
│ Timeline                                  │
│ Personal State                            │
│ Personal Memory                           │
│ Policy / Action Authority                 │
└───────────────┬───────────────┬───────────┘
                │               │
                ▼               ▼
┌─────────────────────┐  ┌──────────────────┐
│ Intelligence Layer  │  │ Integration      │
│                     │  │ Fabric           │
│ Manager             │  │                  │
│ Experts             │  │ Connectors       │
│ Domain Models       │  │ Activepieces     │
└──────────┬──────────┘  │ Native adapters  │
           │             └────────┬─────────┘
           ▼                      │
┌─────────────────────┐           ▼
│ AI Primitives /     │      External World
│ Providers           │
│                     │
│ Local               │
│ Subscription        │
│ API                 │
│ Self-hosted         │
└─────────────────────┘
```

## 중요한 dependency direction

### Domain → primitive

Business domain은 reusable model interface를 사용한다.

Provider가 Health/Memory 의미를 알아서는 안 된다.

### Integration → normalized Floe data

외부 API schema가 Floe Timeline/Memory를 지배하지 않는다.

### Device → provider contract

HealthKit/Health Connect 등 platform API는 Device Provider 뒤에 둔다.

### Intelligence → Action Proposal

Intelligence가 connector mutation을 직접 실행하지 않는다.

### Memory → Evidence

Derived memory는 provenance/evidence로 추적 가능해야 한다.

## Deployment

```text
Device Agent(s)
      │
      ▼
Floe Server
├─ Identity
├─ Personal Core
├─ Intelligence
├─ Integration
├─ Security
└─ Sync
```

일부 Local Sensitive Compute는 서버 밖 Device Agent에 존재한다.

## Implementation Baseline

구체적인 언어/런타임 선택은 `09-implementation/`을 참고한다.

현재 추천 baseline:

```text
Flutter UI
   ↓
Rust Core
   ↓
Swift / Kotlin / OS-native adapters

Server: Go
Database: Turso
Connectors: Native Rust / Go + ConnectorSpec
```

## Expert Ecosystem Boundary

```text
                     Expert Registry
             ┌────────────┼────────────┐
             │            │            │
          Built-in    User-created  Marketplace
             │            │            │
             └────────────┼────────────┘
                          ↓
                    Expert Runtime
                          │
             granted Views / Capabilities
                          │
                          ↓
                       Manager
```

Third-party Experts do not receive direct DB, credential, or unrestricted network access.

The runtime boundary is designed around semantic capabilities and structured outputs.

