# Manager Secretary & Expert Secretaries

> Status: Core intelligence model

## 첫 구현 검증 범위

[S4 Conversational Agent and Expert Foundation](../08-engineering/vertical-slice-delivery.md)는
server와 background lifecycle보다 먼저 local foreground에서 이 구조를 검증한다.

```text
multi-turn text chat
→ bounded Timeline + confirmed Memory views
→ Schedule Expert
→ structured calendar analysis / action proposal
→ Manager synthesis
→ S3 review and action gate
```

built-in Schedule Expert와 deterministic declarative fixture가 동일한 semantic
contract를 사용해야 한다. 이는 Manager/Expert/Tool permission 경계의 검증이며,
Wasm sandbox, Marketplace, 여러 Expert의 동시 orchestration이나 proactive
intervention 완료를 의미하지 않는다.

현재 S4의 Manager-visible `expert.schedule` Tool 표현은 격리된 Expert loop를 검증한
과도기 구현이다. 목표 구조는 [ADR 0018](../../decisions/0018-manager-expert-a2a-delegation.md)의
자연어 A2A delegation이며, Expert를 Tool registry에 등록하지 않는다.

Agent loop, session, registry, budget와 learning port의 구현 계약은
[Agent Runtime and Governed Learning](agent-runtime-and-learning.md)을 따른다.

### Schedule Expert 실행 모델

Schedule Expert는 단순 API wrapper가 아니라 **bounded domain subagent**로 실행한다.
Manager는 사용자 conversation history를 유지하고, Schedule Expert에는 현재
요청에서 파생한 자연어 delegation message와 허가된 View만 전달한다. Expert의 내부
모델 turn은 Manager conversation에 합쳐지지 않는다.

```text
Manager
  → A2A SendMessage("일정 관점에서 …를 검토해줘")
  → lightweight Schedule Expert loop
      ├─ calendar.read/search             # granted calendar evidence
      ├─ schedule.find_free_windows       # optional deterministic analysis
      ├─ playbook.load                    # optional procedural guidance
      └─ model-directed domain judgment
  → Task(completed) + Artifact(natural-language advice + typed references)
  → Manager synthesis
```

Expert는 요청과 현재 근거를 보고 필요한 capability와 Playbook을 임의로 선택한다.
캘린더 내용을 묻는 요청은 일반 read/search만으로 답할 수 있고, 실제 빈 구간 계산이
필요할 때 `schedule.find_free_windows`를 사용할 수 있다. 반복되는 특수 절차는
Playbook으로 분리한다. 예를 들어 focus-time 탐색은 선택 가능한 Calendar Playbook일
뿐 Schedule Expert의 기본 목적이나 공통 실행 순서가 아니다.

호스트는 전체 invocation의 iteration, model/tool call, 시간, token/cost와 output
상한을 집행하지만 Role prompt에 임의의 최소 tool call이나 고정 call sequence를
넣지 않는다. 빈 시간 계산과 interval 병합처럼 정확성이 필요한 연산은 검증 가능한
Rust Tool이 담당한다. Declarative fixture는 LLM 없이 같은 capability/output contract를
검증할 수 있다.

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

Manager Role은 다음 책임과 성공 조건만 안정 지침으로 가진다.

- 사용자의 현재 요청을 이해하고 필요한 근거와 capability를 선택한다.
- 여러 View, Memory와 Expert 결과를 구분하여 종합하고 불확실성을 보존한다.
- 활성 Expert 설명을 바탕으로 독립적인 도메인 판단이 유용할 때만 위임한다.
- Expert에 목표·관련 맥락·제약·기대 결과를 자연어로 전달하되 최종 판단과 사용자
  응답을 소유한다.
- 외부 변경은 typed proposal로 만들고 Policy/Review/Executor 경계를 지킨다.
- 활성 Persona에 따라 일관된 말투로 간결하고 유용하게 응답한다.

특정 tool 호출 순서, 대표 업무 workflow, schema 사본과 runtime budget 숫자는
Manager Role에 넣지 않는다. capability descriptor, Playbook과 host 설정이 각각 이를
소유한다.

### 활성 Expert 조립

Manager의 고정 Role에는 개별 Expert 목록을 넣지 않는다. Context Assembler가 매
model call마다 설치 활성화, Person assignment, 호환성, 권한과 현재 실행 가능성을
확인해 **Active Expert Index**를 scoped instruction으로 조립한다.

```text
Available experts
- Schedule Expert (`floe.schedule`): 일정, 충돌, 시간 가용성과 계획의 현실성을
  검토하는 전문가. 일정 관점의 독립적인 판단이 필요할 때 위임한다.
```

각 항목은 versioned Agent Card의 compact projection인 `id`, `name`, `description`,
`domainTags`만 노출한다. 설명과 coarse skill examples는 도메인과 언제 관점이
유용한지를 짧게 나타내며, 가능한 작업의 완전한 목록, workflow, Tool schema 또는
권한 선언이 아니다. 활성 Expert가 없으면 빈 index를 명시하고 Manager가 존재하지
않는 Expert를 가정하지 못하게 한다.

### In-process A2A

Expert는 현재 별도 서버나 daemon으로 띄우지 않는다. Manager와 같은 프로세스의 A2A
router가 canonical Message/Task/Artifact를 `InProcessA2ATransport`로 Expert Runtime에
전달한다. loopback HTTP나 JSON 직렬화는 하지 않지만, process location을 제외한
discovery, task lifecycle, cancellation과 message semantics는 remote binding과 같다.

```text
A2A Router
├─ InProcessA2ATransport     # 현재 기본
└─ RemoteA2ATransport        # 향후 JSON-RPC/gRPC/HTTP+JSON
```

따라서 Expert를 외부 프로세스로 옮기거나 third-party remote Expert를 연결할 때
Manager 계약을 바꾸지 않는다. 공식 binding과 conformance를 제공하기 전까지는 이를
`A2A-aligned internal runtime`이라고 부르고 A2A compliant server라고 주장하지 않는다.

## Expert

Expert는 API wrapper나 capability가 아니라 **특정 영역의 독립된 판단 Agent**다.

Expert Role은 담당 domain, Manager에게 반환할 판단의 성공 조건, 근거와 불확실성
처리 원칙만 정의한다. Expert는 받은 자연어 assignment를 벗어나 사용자와 직접 대화하거나
권한을 확장하지 않으며, 사용할 수 없는 capability를 가정하지 않는다. 일반 실행은
모델의 판단에 맡기고 반복 가능한 절차가 실제로 필요할 때만 Playbook을 읽는다.

Schedule Expert는 일정의 조회·충돌·가용성·우선순위와 변경 후보를 분석한다. 허가된
Calendar View와 read/search/free-window capability를 필요에 따라 사용해 근거가 있는
자연어 조언을 Artifact로 Manager에게 반환한다. 증거와 변경 후보는 prose 안에
숨기지 않고 typed Data Part reference로 함께 전달한다. 집중 시간 확보는 가능한
요청 유형 중 하나일 뿐 기본 목표가 아니다.

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

Manager가 Expert를 호출하는 것은 Skill 실행이 아니라 Agent-to-Agent delegation이다.
Expert만 자신의 격리된 context에서 허가된 Skill을 실행한다.

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
Agent Directory / Agent Cards
├─ Built-in
├─ User-created
└─ Marketplace
       ↓ compact active projections
Manager → A2A Router
```

Built-in and third-party Experts should share the same semantic concepts:

- Agent Card와 discovery description
- natural-language Message와 Task lifecycle
- trigger
- domain view
- capability dependency
- private state
- Artifact + typed evidence/proposal Data Parts
- permission declaration

Third-party Experts receive more restrictive execution and data-access policy.

See `expert-extension-model.md`.
