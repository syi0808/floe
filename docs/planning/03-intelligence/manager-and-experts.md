# Manager Secretary & Expert Secretaries

> Status: Core intelligence model

## 첫 구현 검증 범위

[S4 Conversational Agent and Expert Foundation](../08-engineering/vertical-slice-delivery.md)는
server와 background lifecycle보다 먼저 local foreground에서 이 구조를 검증한다.

```text
multi-turn text chat
→ bounded Timeline + confirmed Memory views
→ Schedule Expert
→ structured advice / action proposal
→ Manager synthesis
→ S3 review and action gate
```

built-in Schedule Expert와 deterministic declarative fixture가 동일한 semantic
contract를 사용해야 한다. 이는 Manager/Expert/Tool permission 경계의 검증이며,
Wasm sandbox, Marketplace, 여러 Expert의 동시 orchestration이나 proactive
intervention 완료를 의미하지 않는다.

Agent loop, session, registry, budget와 learning port의 구현 계약은
[Agent Runtime and Governed Learning](agent-runtime-and-learning.md)을 따른다.

### Schedule Expert 실행 모델

Schedule Expert는 단순 API wrapper가 아니라 **bounded domain subagent**로 실행한다.
Manager는 사용자 conversation history를 유지하고, Schedule Expert에는 현재
요청에서 파생한 작은 `ScheduleTaskBrief`와 허가된 View만 전달한다. Expert의 내부
모델 turn은 Manager conversation에 합쳐지지 않는다.

```text
Manager
  → ScheduleTaskBrief
  → lightweight Schedule Expert loop
      → schedule.find_free_windows       # deterministic Tool
      → concise domain summary
  → structured ExpertResult
  → Manager synthesis
```

초기 loop는 최대 10번의 작은 model call로 제한한다. 최소 한 번, 최대 9번까지
deterministic scheduling Tool을 호출한 뒤 Manager용 요약으로 종료해야 한다. Tool은
최대 14일의 허가된 View 전체 또는 그 안의 최대 24시간 subrange를 받으므로 여러
날짜 후보를 나눠 비교할 수 있다. 빈 시간 계산, interval 병합과 proposal 후보 생성은
계속 검증 가능한 Rust 로직이 담당한다. Declarative fixture는 LLM 없이 같은
Tool/output contract를 검증할 수 있다.

다일 조회에는 host가 해당 날짜들을 포함하는 bounded Calendar View를 먼저 발급해야
한다. Expert의 반복 호출 자체가 grant를 넓히거나 새로운 Calendar source를 읽을 수는
없다. 현재 Day Canvas Calendar turn은 하루 View를 전달하며, 다일 View transport는
후속 connector increment에서 명시적으로 확장한다.

Expert는 독립적인 사용자-facing personality, 무제한 recursive agent 또는 Calendar
writer가 아니다. Model placement, data class, consent, deadline과 token/cost budget은
호출마다 명시되며, mutation 후보는 계속 Manager와 S3 review/action gate를 통과한다.

## One Assistant

사용자가 대화하는 주체는 하나다.

```text
User
 ↓
Manager Secretary
```

## Manager의 역할

- 사용자와 대화
- 현재 상황 통합
- Expert 의견 취합
- 우선순위 판단
- 계획
- intervention 결정
- relevant memory retrieval
- action proposal 생성
- 사용자에게 전달할 표현 결정

## Expert

Expert는 API wrapper가 아니라 **특정 영역의 판단 서비스**다.

초기 후보:

- Health Expert
- Schedule Expert
- Communication Expert
- Personal Context 관련 전문 로직

## Expert는 항상 떠 있는 Agent가 아니다

모바일에서 daemon처럼 실행되는 모델을 전제로 하지 않는다.

기본은 event-driven.

```text
HealthChanged → Health Expert
CalendarChanged → Schedule Expert
NewEmail → Communication Expert
```

## Expert와 Skill의 차이

```text
Skill = capability
Expert = domain judgment
Manager = overall decision
```

예:

```text
Health Skill
→ health 데이터를 읽는다.

Health Expert
→ 그 데이터가 현재 사용자에게 어떤 의미인지 판단한다.
```

## Expert Memory View

모든 Expert가 전체 Personal Memory를 읽을 필요는 없다.

```text
Full Personal Memory
├─ Manager → broad contextual view
├─ Health Expert → health-relevant projection
├─ Schedule Expert → time/commitments
└─ Communication Expert → communication context
```

## 사용자에게 여러 비서가 보이지 않도록 한다

"건강 비서", "일정 비서", "메일 비서"가 각자 notification을 보내는 UX는 피한다.

Manager가 개입 타이밍과 표현을 통합한다.

## Extensible Expert Layer

Health/Schedule/Communication are built-in Experts, but `Expert` itself is a public extensibility boundary.

```text
Expert Registry
├─ Built-in
├─ User-created
└─ Marketplace
       ↓
Manager
```

Built-in and third-party Experts should share the same semantic concepts:

- trigger
- domain view
- capability dependency
- private state
- structured output
- permission declaration

Third-party Experts receive more restrictive execution and data-access policy.

See `expert-extension-model.md`.
