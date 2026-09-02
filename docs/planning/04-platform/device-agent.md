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

## Open Questions

- Device Protocol의 형태
- offline operation 범위
- local-only Person 가능성
- server authority vs device authority
