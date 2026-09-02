# Manager Secretary & Expert Secretaries

> Status: Core intelligence model

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
