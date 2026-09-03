# Product Principles

> Status: Accepted direction

## 1. Calm by Default

Floe는 정보를 더 많이 보여주는 제품이 아니다.

데이터가 많아질수록 사용자가 보게 되는 것은 오히려 단순해져야 한다.

특히 ADHD 사용자가 기본 화면을 봤을 때 인지 부하와 시각적 산만함이 낮아야 한다.

원칙:

- current time과 가까운 일정이 쉽게 읽혀야 함
- 최소한의 색상
- badge/streak/dashboard 남용 금지
- progressive disclosure
- 중요한 정보만 foreground
- 한 화면에서 정보끼리 경쟁하지 않도록 설계
- 비어 있는 공간을 실패로 보지 않음

## 2. Calendar Is the Canvas

Floe의 기본 화면은 calendar-first다.

Calendar는 단순 connector가 아니라 사용자의 시간과 Floe의 판단이 만나는 primary visual coordinate다.

- Event는 시간축의 기본 구조다.
- scheduled Task는 시간축에 가볍게 결합된다.
- unscheduled Task와 Note는 secondary context로 남는다.
- Floe intervention은 관련 시간/항목에 붙는다.

`Now / Next`는 중요한 product intelligence지만 별도 대형 hero/dashboard를 만들지 않는다. Current-time line, current Event emphasis, compact next navigation처럼 Calendar를 더 잘 읽게 하는 방식으로 표현한다.

## 3. Domain Semantics, Unequal Visual Weight

Event, Task, Note, Commitment, Intervention은 각자의 domain semantics를 유지한다.

Day Canvas에서 모두 보인다고 해서 동일한 row/card template이나 동일한 강조도를 사용하지 않는다.

UI projection은 각 object가 사용자의 하루에서 맡는 역할에 따라 위치와 시각적 무게를 결정한다.

## 4. One Assistant, Many Experts

사용자는 하나의 Floe와 관계를 맺는다.

내부 Health Expert, Schedule Expert, Communication Expert 등이 존재하더라도 기본적으로 직접 사용자에게 각자 말을 걸지 않는다.

```text
Experts → Manager Secretary → User
```

Expert가 만든 insight 역시 별도 Expert dashboard를 요구하지 않는다. Manager가 필요한 경우 Day Canvas나 적절한 system surface에 하나의 Floe intervention으로 표현한다.

## 5. Context Before Conversation

대화는 context를 매번 입력하기 위한 인터페이스가 아니다.

Floe가 이미 알고 있는 Timeline, State, Memory를 바탕으로 사용자의 짧은 질문을 해석해야 한다.

## 6. Capture Without Context Switching

Universal Capture는 사용자가 떠오른 것을 빠르게 맡기고 원래 하던 일로 돌아갈 수 있어야 한다.

초기 PoC에서 explicit classification을 사용할 수 있지만, 장기 제품 경험에서 모든 capture 직후 blocking metadata/classification flow를 강제하지 않는다.

원문은 먼저 안전하게 보존하고, 구조화와 정리는 즉시 또는 나중에 수행할 수 있어야 한다.

## 7. Proactive, but Quiet

개입은 비용이다.

모든 발견을 알리지 않는다.

판단 요소:

- importance
- urgency
- confidence
- actionability
- personal relevance
- attention state
- recent interruption count

Day Canvas 안에서도 Floe를 permanent section으로 노출하기보다 관련 시간/object에 필요한 순간만 나타나게 한다.

## 8. Memory Must Be Inspectable

Floe는 개인사를 기억할 수 있으므로 사용자는 언제든 다음을 할 수 있어야 한다.

- 확인
- 수정
- 삭제
- 출처 확인

## 9. Privacy Is a Product Feature

사용자는 다음을 이해할 수 있어야 한다.

- Floe가 무엇을 알고 있는가
- 어디에 저장하는가
- 외부 AI에 무엇을 보내는가
- 어떤 행동 권한을 가지고 있는가

## 10. Local Where Sensitive

원시 Health, voiceprint, wake-word audio, 민감한 개인 맥락 등은 가능한 한 로컬에서 처리한다.

## 11. Own the Interfaces

Floe의 핵심 abstraction은 Floe가 소유한다.

- Connector interface
- Device provider interface
- Memory model
- Action/Policy boundary

Activepieces 등 외부 생태계는 adapter를 통해 사용한다.

## 12. Experience Parity, not Feature Parity

macOS, Windows, iOS, Android에서 API가 다르더라도 동일한 화면 구성을 억지로 복제하지 않는다.

각 OS가 가장 잘할 수 있는 방식으로 동일한 비서 경험을 달성한다.

Calendar-first mental model은 유지하되 desktop의 grid + Today rail을 mobile에 그대로 축소하지 않는다.

## 13. Open and Self-hostable

서버를 포함한 핵심 시스템은 사용자가 직접 운영 가능한 방향을 유지한다.

Hosted Floe는 OSS stack의 managed distribution에 가깝다.

## 14. Intelligence Does Not Equal Authority

AI가 행동을 판단하는 것과 실제 실행 권한은 분리한다.

## 15. Business Logic Owns Model Choice

어떤 휴리스틱, 어떤 크기의 모델, 어떤 provider를 쓸지는 중앙 Router가 추론해서 정하지 않는다.

그 판단을 가장 잘 이해하는 도메인 컴포넌트가 명시적으로 소유한다.

## 16. Extensible Expertise, Stable Experience

Users can extend what Floe knows how to monitor and advise on without turning Floe into a collection of competing apps.

Third-party Experts integrate through stable data/action contracts while the Manager and Calm UI remain in control of the user experience.

Marketplace Expert가 임의의 dashboard/card/widget를 Day Canvas에 추가하는 것을 기본 확장 방식으로 삼지 않는다.
