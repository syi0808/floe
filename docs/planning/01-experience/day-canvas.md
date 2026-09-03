# Day Canvas

> Status: Core UX

## 목적

Calendar + Todo + Notes를 분리된 앱처럼 보여주지 않고 사용자의 하루 위에 자연스럽게 배치한다.

```text
Wednesday, Sep 2

09:30   출근

10:00   Team Meeting
        준비할 것 1개

12:30   점심

        □ PR 리뷰

14:00   Design Review

15:30   Floe
        오늘은 이 시간을 비워두는 게 좋아 보여요.

18:30   운동

        생각
        "Connector 구조 다시 보기"
```

## UI 원칙

- Now / Next가 가장 강하다.
- 먼 미래는 시각적으로 약하게 한다.
- Event / Task / Note는 의미가 구분되지만 서로 경쟁하지 않는다.
- Floe Intervention은 별도 dashboard가 아니다. 일반 제안은 contextual rail, 좁은 화면에서는 예약된 inline slot을 사용한다. 특정 time block과 직접 관련된 제안은 해당 block 가장자리에 표준 Floe squircle button 하나를 둘 수 있지만 timeline 위의 말풍선으로 표현하지 않는다.
- 데이터 원본의 모든 metadata를 기본 화면에서 보여주지 않는다.
- 검색/입력은 최대한 하나로 유지한다.

상세한 화면 구성과 반응형 동작은 [`docs/design/screens/day-canvas.md`](../../design/screens/day-canvas.md)를 따른다.

## 내부 모델과 UI projection 분리

UI에서 통합되어 보여도 저장 모델은 각각의 semantics를 유지한다.

```text
Timeline Projection
├─ Event
├─ Task
├─ Note
├─ Commitment
└─ Intervention
```

Event의 recurrence/timezone/attendee와 Task의 completion/deadline을 하나의 generic schema에 억지로 합치지 않는다.

## Now / Next

장기적으로 Day Canvas의 기본 viewport는 하루 전체보다 사용자의 현재 시간 주변을 중심으로 할 수 있다.

예:

- 지금 하고 있는 것
- 바로 다음 일정
- 그 전에 해야 할 한두 가지
- 중요한 Floe 제안

## Open Questions

- Day Canvas의 정확한 시간축 표현
- 시간 없는 Todo/Note의 배치 방법
- 모바일과 데스크톱의 동일 모델/다른 표현 범위
- 일정이 과도하게 많은 날의 folding 기준
