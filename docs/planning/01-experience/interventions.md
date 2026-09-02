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

## Expert → Manager

Expert가 insight를 생성했다고 바로 사용자에게 전달하지 않는다.

```text
Health Expert
Schedule Expert
Communication Expert
       ↓
     Manager
       ↓
interruption decision
       ↓
      User
```

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
