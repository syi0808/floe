# Device Agent

> Status: Core platform abstraction

## 정의

각 OS 앱은 단순 UI client가 아니다.

해당 기기에서만 가능한 capability와 sensitive local compute를 Floe에 연결하는 **Device Agent**다.

## 책임 후보

```text
Device Agent
├─ Native UI
├─ Secure Storage
├─ Local Inference
├─ Voice
├─ Notifications
├─ OS Events
├─ Health
├─ Local Context
└─ Sync
```

## Provider Abstraction

Core가 OS-specific API를 직접 알지 않도록 provider contract를 둔다.

후보:

```text
HealthProvider
CalendarProvider
ContactProvider
VoiceProvider
NotificationProvider
LocationProvider
SecureStorageProvider
LocalModelProvider
InvocationProvider
```

예:

```text
HealthProvider
├─ AppleHealthKitProvider
└─ AndroidHealthConnectProvider
```

## Device-local Secret

다음은 가능한 한 Device Agent secure storage에 둔다.

- voiceprint
- wake-related state
- provider subscription credentials
- device key
- 일부 sensitive memory key material

## Server와의 관계

Device Agent는 서버의 thin client가 아니라 독립적인 compute/privacy boundary다.

Server가 없어도 일부 로컬 기능이 동작할 수 있도록 설계 여지를 남긴다.

## Context Policy

Device Agent는 raw OS/provider data를 곧바로 sync하지 않는다. source별 observation을
로컬에서 provider-neutral View로 정규화·축소하고, purpose/freshness/transfer policy가
허용한 View만 다른 기기 또는 서버에 제공한다.

- Location과 Attention은 현재 interaction/presence device에 묶인 device-scoped context다.
- raw Health, raw Screen Time/activity와 precise location은 device-only다.
- cross-device 최신 context는 일반 DB 복제가 아니라 만료되는 bounded query lease로 읽는다.
- device-native Act는 해당 권한을 가진 execution owner가 수행한다.
- macOS Attention은 public activity heuristic이며 Apple Screen Time으로 표시하지 않는다.

상세 source matrix, routing, freshness, disagreement와 S5.5/S8 경계는
[ADR 0024](../../decisions/0024-device-context-collection-and-convergence.md)를 따른다.

## Remaining Questions

- Device Protocol의 wire encoding과 transport
- offline operation 범위
- local-only Person 가능성
- end-to-end key recovery와 opaque relay 구현
