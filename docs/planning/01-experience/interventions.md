# Interventions

> Status: Core behavior model

## 정의

Floe의 핵심 행동 primitive.

```text
Observe
 ↓
Understand
 ↓
Predict
 ↓
Intervene
```

`Intervene`는 insight를 만든다는 뜻이 아니라 사용자의 attention을 사용할 가치가
있다고 Manager와 policy가 별도로 결정했다는 뜻이다. Background Expert 결과의 기본
delivery는 UI가 아니라 **silence/defer**다.

## 종류

### Inform

"다음 일정까지 20분 남았습니다."

### Warn

"지금 출발하면 약속에 늦을 가능성이 있습니다."

### Suggest

"오늘 운동 강도를 낮추는 편이 좋아 보여요."

### Prepare

"회의를 4시로 옮길 수 있도록 준비했습니다."

### Act

"회의를 4시로 옮겼습니다."

## Intervention Budget

개입 자체를 비용으로 취급한다.

판단 후보:

```text
importance
urgency
confidence
actionability
personal relevance
attention state
recent interruption count
```

## UI presentation hierarchy

Intervention이 사용자에게 전달되기로 결정된 뒤에는 먼저 channel과 표현 강도를
선택한다.

```text
silence / defer
  ↓ delivery is justified
voice report | quiet notification | passive visual entry
  ↓ more context or a decision is required
structured report / comparison / approval UI
  ↓ consequential mutation
explicit transaction-bound confirmation
```

- 음성은 장기적인 기본 conversational channel이며 visual UI보다 먼저 고려한다.
- UI는 consent, consequential approval, 복잡한 비교, provenance, recovery와 audit에
  실질적인 가치가 있을 때만 연다.
- 한 viewport에서 능동 제안은 최대 1개만 확장한다.
- overview content 위에 mascot 말풍선을 띄우지 않는다. 단, 특정 time block과 직접 관련된 제안은 표준 icon-only Floe squircle button 하나로 anchor할 수 있다.
- 제안이 있다는 이유만으로 modal을 자동으로 열지 않는다.
- desktop은 contextual rail, narrow layout은 예약된 inline slot 또는 sheet를 사용한다.
- 상세 component와 상태는 [`docs/design/assistant-and-interventions.md`](../../design/assistant-and-interventions.md)를 따른다.

## Expert → Manager

Expert가 insight를 생성했다고 바로 사용자에게 전달하지 않는다.

```text
Wellbeing Expert
Schedule & Feasibility Expert
Commitments / Communication Expert
       ↓
     Manager
       ↓
interruption decision
       ↓
     User
```

Expert는 notification이나 음성을 직접 발송하지 않는다. Manager는 여러 Expert 결과와
현재 conversation, attention state, recent interruption을 합쳐 delivery를 선택한다.
서로 연관된 결과는 한 번의 비서 보고로 합치며 provider/Expert별 알림으로 분리하지
않는다.

## 학습 가능성

사용자가 반복적으로:

- 무시한다
- dismiss한다
- accept한다
- 직접 설정을 바꾼다

와 같은 feedback을 주면 intervention threshold를 개인화할 수 있다.

## 비목표

Floe는 Notification Machine이 아니다.

"조용히 있는 것"도 intelligence로 본다.
