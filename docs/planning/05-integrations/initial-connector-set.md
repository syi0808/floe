# Initial Connector Set

> Status: Recommended initial product scope

## Goal

초기 Floe의 핵심 루프를 완성하는 데 필요한 connector만 먼저 구현한다.

기준은 다음 네 가지다.

1. **하루를 이해한다** — Calendar
2. **커뮤니케이션에서 해야 할 일과 약속을 발견한다** — Mail
3. **사람을 올바르게 식별하고 관계 Memory와 연결한다** — Contacts
4. **사용자의 컨디션을 이해한다** — Health

Todo와 Notes는 초기에는 Floe-native domain으로 시작한다.
Voice, Location, Notification은 Connector가 아니라 Device Provider로 분리한다.

---

# 1. Initial Logical Domains

```text
Connector Domains
├─ Calendar
├─ Mail
├─ Contacts
└─ Health

Floe-native Domains
├─ Tasks
└─ Notes

Device Providers
├─ Voice
├─ Location
├─ Notifications
├─ Secure Storage
└─ Invocation
```

이 구분을 통해 integration 범위를 불필요하게 넓히지 않는다.

---

# 2. Concrete Connector Set

## Stage A — macOS/iOS Dogfood

### Apple Calendar

**Implementation:** Device-native

```text
EventKit
  ↓
Swift Adapter
  ↓
Rust Device Core
  ↓
Calendar Connector
```

역할:

- calendar list
- event bootstrap
- event changes
- event create/update/delete
- recurrence/timezone/attendee metadata

초기 권한은 read/write가 필요한 시점에 요청한다.

### Apple Contacts

**Implementation:** Device-native

역할:

- contact identity read
- name / alias / email / phone linkage
- Personal Memory의 Person identity resolution 보조

초기에는 read-only.

Contacts 자체를 Personal Memory로 복사하는 것이 아니라 `ExternalIdentity` evidence로 사용한다.

### Gmail

**Implementation:** Server-native Go connector

초기 역할:

- search
- message/thread metadata
- body read on demand
- mailbox changes
- Communication Expert input
- commitment / event / people Memory Candidate extraction

초기에는 **메일 전송보다 읽기/검색/변화 감지**를 우선한다.

Draft와 Send는 Action Authority가 안정된 뒤 추가할 수 있다.

### HealthKit

**Implementation:** Device-native, iOS/iPadOS source

```text
HealthKit Raw Data
      ↓
Local Health Engine
      ↓
Heuristic / Tiny Model
      ↓
Derived Health State
      ↓
Floe State Sync
```

macOS에서는 HealthKit store를 직접 읽는 것을 전제로 하지 않는다.
건강 데이터는 iPhone/iPad Device Agent에서 공급한다.

초기 read type 후보:

- sleep
- resting heart rate
- heart rate variability where available
- step/activity
- active energy
- workout/exercise session

Raw Health data는 server connector payload가 아니다.

---

## Stage B — Cross-platform Completion

### Google Calendar

**Implementation:** Server-native Go connector

역할:

- calendar list
- initial event sync
- incremental sync
- webhook/poll change detection
- create/update/delete

Google Calendar를 직접 연결한 경우 해당 Google calendars는 OS Calendar connector에서 중복 ingest하지 않는 것을 기본 정책으로 한다.

### Android Calendar

**Implementation:** Device-native

```text
Calendar Provider / CalendarContract
        ↓
Kotlin Adapter
        ↓
Rust Device Core
```

직접 Google/Microsoft connector로 이미 관리되는 calendar는 기본적으로 제외한다.

주요 목적:

- local/device-only calendar
- direct SaaS connector가 없는 calendar source
- Android-native calendar integration

### Android Contacts

**Implementation:** Device-native

Apple Contacts와 동일한 logical Contact Connector contract를 구현한다.

### Health Connect

**Implementation:** Device-native

```text
Health Connect
     ↓
Kotlin Adapter
     ↓
Local Health Engine
     ↓
Derived Health State
```

HealthKit과 동일한 상위 `HealthState`를 만든다.
플랫폼 raw schema를 Floe domain으로 그대로 노출하지 않는다.

### Microsoft Calendar / Mail

**Implementation:** Server-native Go connectors

Microsoft identity/auth는 공유할 수 있지만 logical connector는 분리한다.

```text
Microsoft ExternalAccount
├─ microsoft.calendar
└─ microsoft.mail
```

이는 Windows/Outlook 중심 사용자를 1급으로 지원하기 위한 첫 확장이다.

---

# 3. Explicitly Not Initial

다음은 가치가 없어서가 아니라 initial connector surface를 안정시키기 위해 뒤로 미룬다.

## External Task Connectors

- Apple Reminders
- Google Tasks
- Microsoft To Do
- Todoist

초기 Floe Task가 canonical task domain이다.

향후 interop/import/sync 요구가 검증되면 추가한다.

## External Note Connectors

- Apple Notes
- Google Keep
- OneNote
- Notion as a note source

초기 Floe Note가 canonical note domain이다.

특히 Apple Notes는 안정적인 범용 connector API가 제한적이므로 초기 핵심 의존성으로 두지 않는다.

## Generic IMAP

Gmail/Outlook direct connector가 먼저다.

IMAP은 provider coverage를 넓힐 수 있지만:

- auth variants
- OAuth provider quirks
- push/IDLE lifecycle
- sending SMTP
- thread semantics

등의 비용 때문에 초기에는 제외한다.

---

# 4. Execution Placement

Connector는 domain보다 **데이터 소유 위치와 lifecycle**에 따라 실행 위치를 결정한다.

| Connector | Placement | Why |
|---|---|---|
| Apple Calendar | Device | OS permission / local calendars |
| Android Calendar | Device | OS Calendar Provider |
| Apple Contacts | Device | private local identity data |
| Android Contacts | Device | private local identity data |
| HealthKit | Device | raw sensitive health data |
| Health Connect | Device | raw sensitive health data |
| Gmail | Server | always-online sync/search/webhook |
| Google Calendar | Server | always-online sync/actions |
| Microsoft Mail | Server | always-online sync/search |
| Microsoft Calendar | Server | always-online sync/actions |

Placement는 UI 위치와 다르다.
Flutter는 connector 실행 runtime이 아니다.

---

# 5. Source-of-Truth Policy

## Calendar

외부 calendar event의 source-of-truth는 해당 provider다.

Floe는 local/server mirror와 Timeline projection을 가진다.

```text
External Calendar
       ↓
Connector Mirror
       ↓
Timeline Projection
```

Floe에서 event를 수정할 경우:

```text
User / Manager
      ↓
Action Proposal
      ↓
Policy
      ↓
Connector write
      ↓
External Provider
      ↓
change sync
      ↓
Floe mirror updated
```

Floe DB만 먼저 canonical mutation하고 provider에 나중에 맞추는 구조를 기본으로 하지 않는다.

## Mail

메일 provider가 source-of-truth다.

Floe는 전체 mailbox archive를 만드는 것을 목표로 하지 않는다.

저장 후보:

- message/thread identifiers
- headers / metadata needed for search/context
- extracted structured facts
- derived commitments/events
- provenance

Body는 필요할 때 provider에서 읽고, retention 정책에 따라 transient/encrypted cache만 둘 수 있다.

## Contacts

OS/provider contact store가 contact record의 source-of-truth다.

Floe `Person`은 contact record 자체가 아니다.

```text
Floe Person
   │
   ├─ ContactIdentity(Apple)
   ├─ ContactIdentity(Android)
   ├─ EmailIdentity
   └─ other aliases
```

## Health

HealthKit / Health Connect가 raw signal의 source-of-truth다.

Floe는 raw health mirror가 아니라 derived Personal State를 보존한다.

---

# 6. Calendar Route Arbitration

같은 Google/Microsoft calendar가 OS Calendar와 direct API 양쪽에 보이는 경우가 있다.

이를 동시에 canonical ingestion하지 않는다.

## Preferred Route

```text
Google Calendar available directly
→ Google Calendar connector preferred

Microsoft Calendar available directly
→ Microsoft connector preferred

iCloud / local / unsupported provider
→ OS Calendar connector
```

## Why

Direct server connector는:

- device offline에서도 동작
- webhook/poll sync 가능
- OAuth account identity가 명확
- server Manager가 action 가능

이라는 장점이 있다.

OS connector는 direct API가 없는 source를 보완한다.

## Initial Dedup UX

자동 account/calendar identity mapping이 불확실하면 magic dedup을 하지 않는다.

연결 시 사용자가 calendar source를 선택하게 한다.

```text
Calendars Floe will use

Google — Personal        [Direct] ✓
Google — Work            [Direct] ✓
iCloud — Home            [This Mac] ✓
Google — Personal        [This Mac] excluded
```

나중에 provider/account fingerprint mapping을 강화할 수 있다.

---

# 7. Connection Model

`ConnectorConnection`에 실행 위치와 외부 계정 개념을 명시한다.

```text
ExternalAccount {
  id
  personId
  provider
  providerSubject
  displayIdentity
  credentialRef
  grantedScopes
}
```

예:

```text
Google Account
├─ Gmail ConnectorConnection
└─ Google Calendar ConnectorConnection
```

Device-native connector는 ExternalAccount가 없을 수 있다.

```text
ConnectorConnection {
  id
  personId
  connectorId

  execution:
    Server
    Device(deviceId)

  externalAccountId?
  selectedResources
  capabilitySet
  syncStateRef
  status
}
```

Credential value 자체는 이 record에 넣지 않는다.

---

# 8. Connector Change Model

Vendor-specific change payload를 Floe Core에 직접 흘리지 않는다.

최소 envelope:

```text
ConnectorChange {
  connectionId
  sourceRef
  domain
  operation
  externalRevision?
  occurredAt?
  payload
}
```

`sourceRef`:

```text
SourceRef {
  connectionId
  resourceType
  externalId
  externalRevision?
}
```

Calendar / Mail / Contact 등 domain projector가 payload를 해당 domain으로 변환한다.

---

# 9. Connector Data Policies

## Calendar

Server sync: allowed

Persist:

- normalized event
- external identity/revision
- attendee identity references

## Mail

Server processing: allowed when user grants it and provider policy permits

Default persistence:

- metadata/index
- structured derived information
- provenance

Full body long-term archive: not default

External email content is always untrusted input for AI/memory.

## Contacts

Persist only fields needed for identity resolution and user-visible contact linkage.

Do not silently copy the entire address book into long-term semantic memory.

## Health

Raw data:

- device only by default

Server receives:

- sanitized/derived state only

---

# 10. Change Delivery Strategies

The connector contract must not assume every provider supports webhook push.

```text
ChangeDelivery
├─ OS event
├─ Webhook
├─ Polling
└─ Scheduled local refresh
```

Examples:

### Apple Calendar

OS EventKit change notification → re-query.

### Google Calendar

Hosted Floe:

```text
push notification
→ incremental sync token query
```

Self-host without public webhook:

```text
scheduled incremental sync
→ sync token
```

### Gmail

Hosted Floe:

```text
Gmail watch / Pub/Sub
→ historyId
→ history.list
```

Self-host baseline can initially use polling when Google Pub/Sub setup would damage the self-host UX.

### HealthKit

Observer/background delivery where available.

### Health Connect

foreground/background scheduled reads according to platform capability.

---

# 11. Initial Capability Surface

## Calendar Connector

```text
calendar.collections.list
calendar.events.bootstrap
calendar.events.changes
calendar.events.read
calendar.events.create
calendar.events.update
calendar.events.delete
```

Delete exists as connector capability but remains a high-authority action.

## Mail Connector

Initial:

```text
mail.search
mail.threads.read
mail.messages.read
mail.changes
```

Later:

```text
mail.draft.create
mail.send
mail.archive
```

## Contact Connector

```text
contacts.list
contacts.read
contacts.changes?
```

Initial write is unnecessary.

## Health Connector

Connector-level output should mainly be local health signals/changes consumed by the Health Engine.

Raw generic `health.sample.read` API does not need to be exposed to the server Manager.

---

# 12. Initial Implementation Order

## P0 — macOS Daily Loop

1. Apple Calendar
2. Apple Contacts
3. Gmail read/search/change
4. Floe-native Task/Note

This validates:

```text
Calendar
+
People
+
Communication
+
Day Canvas
+
Personal Memory
```

## P0.5 — Health Loop

5. iOS HealthKit companion connector

This validates:

```text
Health raw data
→ local derived state
→ Manager schedule intervention
```

## P1 — Direct Calendar Reliability

6. Google Calendar direct connector

If the primary dogfood calendar is Google, this moves into P0 because it enables always-online schedule management.

## P1 — Android/Windows Completion

7. Health Connect
8. Android Calendar
9. Android Contacts
10. Microsoft Calendar
11. Microsoft Mail

---

# 13. Success Criteria

초기 connector architecture가 성공하려면 다음이 가능해야 한다.

### Scenario A — Day Planning

```text
Calendar connector
→ events
→ Schedule Expert
→ Day Canvas
```

### Scenario B — Email Commitment

```text
Gmail
→ new/relevant message
→ Communication Expert
→ commitment candidate
→ Personal Memory / Task
```

### Scenario C — Person Context

```text
Email sender / calendar attendee
+
Contacts
→ Floe Person resolution
```

### Scenario D — Health-aware Schedule

```text
HealthKit / Health Connect
→ local health state
+
Calendar
→ Health/Schedule Experts
→ Manager intervention
```

이 네 루프가 완성되지 않는 connector는 initial scope에서 우선순위를 낮춘다.
