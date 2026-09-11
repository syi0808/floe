# Sync & Multi-device

> Status: High-risk design area

> Context collection, execution ownership, query lease, freshness and convergence semantics are
> fixed by [ADR 0024](../../decisions/0024-device-context-collection-and-convergence.md). Storage,
> transport and key-recovery implementation remain S8 PoC decisions.

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

## Context Is Not One Sync Stream

Cross-device data movement uses three distinct mechanisms.

| Mechanism | 대상 | 예시 |
| --- | --- | --- |
| durable convergence | Floe canonical record, normalized mirror, tombstone | Timeline, confirmed Memory, Calendar mirror, Review result |
| encrypted derived snapshot | 명시적으로 허용된 bounded derived state | coarse Wellbeing state |
| expiring query lease | 빠르게 변하거나 device presence에 묶인 context | Location, ETA, Attention |

Raw Health/Screen Time/activity, precise location, credential과 Temporary AI Context는 durable
sync 대상이 아니다. query lease는 producer device가 local privacy projection을 수행한 뒤
authorized consumer에게만 전달하며, cross-device permission과 remote-model transfer consent를
분리한다.

Source route는 timestamp 하나로 선택하지 않는다. source-of-truth, selected scope, execution
owner, interaction/device presence, freshness와 health를 적용하고, disagreement를 보존한다.
Location과 Attention은 device-scoped이며 여러 기기의 값을 자동 평균하지 않는다.

Go server는 durable sync 외에 Device Gateway와 capability directory를 제공한다. Local Agent의
cross-device read는 server를 통한 deadline-bounded lease이며, producer가 online이고 OS가 실행을
허용할 때만 fresh 응답을 보장할 수 있다. Highly Sensitive projection은 server가 해독하지 않는
opaque relay가 기본이다.

모든 device context를 주기적으로 적재하지 않는다. 주기 heartbeat는 presence/capability health
metadata만 갱신한다. canonical/mirror record는 change delta로, 허용된 derived state는 최신
encrypted snapshot 하나로, Location/ETA/Attention은 content를 저장하지 않는 query lease로
전달한다.

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
- device directory/lease transport
- clock-skew bound and relay receipt protocol
