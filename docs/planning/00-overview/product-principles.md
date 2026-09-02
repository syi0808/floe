# Product Principles

> Status: Accepted direction

## 1. Calm by Default

Floe는 정보를 더 많이 보여주는 제품이 아니다.

데이터가 많아질수록 사용자가 보게 되는 것은 오히려 단순해져야 한다.

특히 ADHD 사용자가 기본 화면을 봤을 때 인지 부하와 시각적 산만함이 낮아야 한다.

원칙:

- Now / Next 중심
- 최소한의 색상
- badge/streak/dashboard 남용 금지
- progressive disclosure
- 중요한 정보만 foreground
- 한 화면에서 정보끼리 경쟁하지 않도록 설계

## 2. One Assistant, Many Experts

사용자는 하나의 Floe와 관계를 맺는다.

내부 Health Expert, Schedule Expert, Communication Expert 등이 존재하더라도 기본적으로 직접 사용자에게 각자 말을 걸지 않는다.

```text
Experts → Manager Secretary → User
```

## 3. Context Before Conversation

대화는 context를 매번 입력하기 위한 인터페이스가 아니다.

Floe가 이미 알고 있는 Timeline, State, Memory를 바탕으로 사용자의 짧은 질문을 해석해야 한다.

## 4. Proactive, but Quiet

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

## 5. Memory Must Be Inspectable

Floe는 개인사를 기억할 수 있으므로 사용자는 언제든 다음을 할 수 있어야 한다.

- 확인
- 수정
- 삭제
- 출처 확인

## 6. Privacy Is a Product Feature

사용자는 다음을 이해할 수 있어야 한다.

- Floe가 무엇을 알고 있는가
- 어디에 저장하는가
- 외부 AI에 무엇을 보내는가
- 어떤 행동 권한을 가지고 있는가

## 7. Local Where Sensitive

원시 Health, voiceprint, wake-word audio, 민감한 개인 맥락 등은 가능한 한 로컬에서 처리한다.

## 8. Own the Interfaces

Floe의 핵심 abstraction은 Floe가 소유한다.

- Connector interface
- Device provider interface
- Memory model
- Action/Policy boundary

Activepieces 등 외부 생태계는 adapter를 통해 사용한다.

## 9. Experience Parity, not Feature Parity

macOS, Windows, iOS, Android에서 API가 다르더라도 동일한 UX를 억지로 복제하지 않는다.

각 OS가 가장 잘할 수 있는 방식으로 동일한 비서 경험을 달성한다.

## 10. Open and Self-hostable

서버를 포함한 핵심 시스템은 사용자가 직접 운영 가능한 방향을 유지한다.

Hosted Floe는 OSS stack의 managed distribution에 가깝다.

## 11. Intelligence Does Not Equal Authority

AI가 행동을 판단하는 것과 실제 실행 권한은 분리한다.

## 12. Business Logic Owns Model Choice

어떤 휴리스틱, 어떤 크기의 모델, 어떤 provider를 쓸지는 중앙 Router가 추론해서 정하지 않는다.

그 판단을 가장 잘 이해하는 도메인 컴포넌트가 명시적으로 소유한다.
