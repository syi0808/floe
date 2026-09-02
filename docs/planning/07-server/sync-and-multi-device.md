# Sync & Multi-device

> Status: High-risk design area

## 목표

같은 Person의 Floe가 macOS, Windows, iOS, Android에서 일관된 Timeline/Memory를 제공해야 한다.

## 주요 난점

- offline edit
- concurrent edit
- external source update
- external deletion
- recurrence changes
- duplicate ingestion
- tombstone
- revision
- device clock
- partial connectivity

## Source of Truth

같은 외부 event를:

```text
Google Calendar API → Floe
EventKit mirror → Floe
```

두 경로에서 동시에 canonical ingestion하지 않도록 한다.

## Mutation Log

장기적으로 append/mutation log 기반 sync를 검토할 가치가 있다.

중요한 것은 exact 기술보다:

- idempotency
- revision validation
- replay 가능성
- delete propagation

을 만족하는 것이다.

## Sensitive Data

모든 데이터가 같은 sync 정책을 쓰지 않을 수 있다.

예:

- voiceprint: device-only
- raw Health: device-only
- Timeline: encrypted sync
- Personal Memory: 강한 encryption / policy-dependent
- temporary AI context: sync하지 않음

## Device Arbitration

"Floe" 호출을 여러 기기가 동시에 듣는 복잡한 arbitration은 초기 핵심 요구에서 제외한다.

초기 규칙 예:

- explicit invocation → 호출된 device가 응답
- ambient wake → macOS/Windows desktop 위주
- notification action → interaction한 device가 응답

## Open Questions

- sync storage engine
- CRDT 필요성
- end-to-end encryption 범위
- key recovery
- multi-person instance isolation
