# Assistant Context Portfolio

> Status: Accepted source priorities for S4 and follow-up delivery

## Decision method

Connector 수 자체가 목표가 아니다. Floe 비서가 다음 질문에 근거 있게 답할 수
있는지를 기준으로 source를 선택한다.

1. **Time:** 오늘 무엇이 있고 언제 비는가?
2. **Commitments:** 무엇을 답하거나 준비하기로 했는가?
3. **People:** 누구와 관련된 일이며 어떤 맥락인가?
4. **Feasibility:** 언제 출발해야 하고 외부 조건은 무엇인가?
5. **Capacity:** 지금 집중·회복 여력이 어떤가?
6. **Execution:** 제안한 일을 안전하게 준비하거나 실행할 수 있는가?

각 후보는 assistant value, source authority, public API viability, privacy,
local/server placement, freshness, failure isolation과 maintenance cost로 평가한다.

## S4 required source cohort

S4의 대표 시나리오는 **Today briefing / next-event preparation**이다.

```text
Calendar             → 오늘 일정과 빈 시간
Gmail                → 답변/준비가 필요한 thread
Contacts             → sender/attendee identity evidence
Location + Map ETA   → 다음 장소까지 출발 가능 시각
Weather              → 이동/준비에 영향을 주는 외부 조건
Apple Health         → local-derived capacity signal
Screen Time gate     → 가능할 때만 local-derived attention signal
                         ↓
            Schedule / Communication / Health Experts
                         ↓
                    Manager briefing
                         ↓
               grounded answer / S3 proposal
```

### Required live integrations

| Source | Runtime class | Retention | S4 value |
| --- | --- | --- | --- |
| Apple Calendar | device connector | normalized mirror | schedule authority and action target |
| Gmail | local Go connector | index + on-demand body | commitments and preparation |
| Apple Contacts | device connector | identity reference | sender/attendee resolution |
| Core Location | device context provider | ephemeral/derived | current-origin feasibility |
| MapKit ETA | device context provider | short-lived derived | leave-by calculation |
| WeatherKit | device/server context provider | short-lived cache | travel and day constraints |
| Apple Health | iOS/iPadOS sensitive provider | derived only | coarse capacity/recovery context |

### Required feasibility gate

Screen Time uses only public Family Controls/Device Activity APIs. S4 must test
authorization, entitlement, region and whether a coarse attention signal can legally
leave the extension/provider boundary. Unsupported is a valid architecture result;
private database access or undocumented APIs are not alternatives.

### Floe-native inputs, not connectors

- Task and Note canonical stores
- Review decisions and Activity outcomes
- AgentSession and user corrections
- notification delivery and voice invocation Device Providers

These participate in the same briefing but do not pretend to be external connectors.

## S4 minimum information policy

- Calendar: event time, source, availability, location needed for the selected range.
- Gmail: bounded message/thread IDs and headers; body only for user query or Expert
  request with `mail.messages.read`.
- Contacts: limited-access selection is supported; no contact note field; records are
  identity evidence, not Personal Memory.
- Location: When In Use and reduced accuracy are sufficient initially; no location
  history or Always permission.
- ETA: request only for a visible next-event question; do not build movement history.
- Weather: current/hourly conditions needed for the event window with required
  attribution; no indefinite raw forecast archive.
- Apple Health: selected read types processed locally into coarse, non-diagnostic
  state; raw samples do not enter Agent context or server payload.
- Screen Time: derived aggregate only; no raw app/domain timeline in Agent context.

## Follow-up portfolio

### Tier B — provider parity and work context

- Google Calendar direct connector
- Microsoft Outlook Calendar and Mail
- Microsoft/Google contacts where needed
- Slack and Microsoft Teams selected-channel/thread read
- Google Drive, OneDrive and Dropbox file search/read on demand
- Apple Reminders, Google Tasks and Microsoft To Do import after Floe Task conflict
  semantics are stable

These expand the same Time/Commitments/People questions. They should reuse S4 View
contracts rather than add provider-specific Expert prompts.

### Tier C — mobility, home and specialized context

- transit/ride-hailing and flight status providers
- package delivery and reservation extraction, initially through mail evidence
- Home Assistant and supported smart-home state/actions
- additional Health Connect and fitness providers
- project systems such as GitHub, Linear, Jira and Notion for explicitly selected
  workspaces

### Not initial assistant context

- banking/payment or full financial transaction feeds
- password-manager vault contents
- raw browser history, screen recording or continuous screenshot ingestion
- clinical records and diagnosis-oriented health sources
- full address-book copy into Personal Memory
- unrestricted Slack/Teams organization history
- iMessage/SMS/private app data without a supported public API
- arbitrary webhook payloads promoted directly into Agent context

These have high sensitivity, weak initial Day-assistant value or unsafe authority
boundaries. A concrete user scenario and separate security decision are required.

## Portfolio rule

S4 does not require every Tier B/C source. It does require the common connection,
capability, View, provenance and failure contracts to make the next provider an
adapter rather than an Agent-runtime change. A connector is accepted only when at
least one Expert/user scenario consumes its bounded View and the disconnected state
remains useful and understandable.

Before S8, source-cohort composition is tested with contract fixtures while each
live source is accepted on its native execution host. S4 does not claim that an
iPhone-only Health/Screen Time View already appears in a macOS briefing; that claim
requires the S8 cross-device boundary.

## Official platform constraints

- [Gmail messages.list/get](https://developers.google.com/workspace/gmail/api/guides/list-messages)
- [Apple Contacts authorization and limited access](https://developer.apple.com/documentation/contacts/accessing-the-contact-store)
- [Core Location authorization](https://developer.apple.com/documentation/corelocation/requesting-authorization-to-use-location-services)
- [MapKit directions and ETA](https://developer.apple.com/documentation/mapkit/mkdirections)
- [WeatherKit and attribution](https://developer.apple.com/weatherkit/)
- [Screen Time technology frameworks](https://developer.apple.com/documentation/ScreenTimeAPIDocumentation)
- [FamilyActivityData region/entitlement limits](https://developer.apple.com/documentation/familycontrols/familyactivitydata)
- [HealthKit device availability](https://developer.apple.com/documentation/healthkit/about-the-healthkit-framework)
