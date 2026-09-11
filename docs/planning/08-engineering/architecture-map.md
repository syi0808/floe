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
│ Manager Agent Loop  │  │                  │
│ Tool Registry       │  │ Connectors       │
│ Agent Directory/A2A │  │                  │
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

### Context/Event → Manager → Interact

허가된 connector와 device provider 변화는 bounded Situation 후보가 된다. Manager가
domain Expert 판단을 종합하고 intervention policy를 통과시킨 뒤에만 voice,
notification 또는 visual UI로 전달한다. UI process가 background intelligence의
소유자가 되어서는 안 된다.

### Integration → normalized Floe data

외부 API schema가 Floe Timeline/Memory를 지배하지 않는다.
Integration은 read/write를 하나의 암묵적 권한으로 묶지 않고 provider-neutral Observe
View와 Act capability로 분리한다. Voice/notification/UI는 data connector가 아니라
Interact provider다.

### Device → provider contract

HealthKit/Health Connect 등 platform API는 Device Provider 뒤에 둔다.

### Intelligence → Action Proposal

Intelligence가 connector mutation을 직접 실행하지 않는다.

### Interface → AgentCommand/Event

Chat, voice와 future server transport는 같은 versioned AgentCommand/Event를 사용한다.
Flutter callback이나 provider response schema가 Agent core contract가 되어서는 안 된다.

### Learning → Candidate

Agent self-improvement는 Personal Memory 또는 procedural Playbook candidate를 만들 뿐,
identity, safety policy, permission이나 model weights를 직접 변경하지 않는다.

### Memory → Evidence

Derived memory는 provenance/evidence로 추적 가능해야 한다.

## Deployment

```text
Device Agent(s)
      │
      ▼
Floe Server
├─ Identity
├─ Device / Capability Directory
├─ ContextQuery Gateway / Opaque Relay
├─ Personal Core
├─ Intelligence
├─ Integration
├─ Security
└─ Durable Sync
```

일부 Local Sensitive Compute는 서버 밖 Device Agent에 존재한다.
Location/ETA/Attention 같은 fast context는 일반 sync DB가 아니라 만료되는 query lease로
요청하며, cross-Person context는 owner가 허용한 Shared View만 전달한다. source별 collection,
freshness, routing과 transfer 경계는
[ADR 0024](../../decisions/0024-device-context-collection-and-convergence.md)를 따른다.

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
                  Agent Directory / Agent Cards
             ┌────────────┼────────────┐
             │            │            │
          Built-in    User-created  Marketplace
             │            │            │
             └────────────┼────────────┘
                          ↓
                 A2A Router / Expert Runtime
                          │
          in-process binding + granted Views / Tools
                          │
                          ↓
                       Manager
```

Third-party Experts do not receive direct DB, credential, or unrestricted network access.

The runtime boundary uses A2A-aligned Message, Task and Artifact semantics. Experts
normally share the Manager process through an in-process binding; remote bindings can
be added without turning Experts into Tools.
