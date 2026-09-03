# Day Canvas

> Status: Core UX — calendar-first baseline

## 정의

**Calendar is the canvas. Tasks, notes, and Floe are layers on top of it.**

Floe의 기본 화면은 "오늘 요약 대시보드"가 아니라 사용자가 이미 익숙하게 이해할 수 있는 **시간 기반 Calendar view**여야 한다.

Calendar + Todo + Notes를 별도 앱으로 분리하지 않되, 세 도메인을 시각적으로 동등한 목록 항목으로 취급하지도 않는다.

- Event는 **시간 공간을 차지하는 기본 구조**다.
- Task는 **해야 할 일**이며, 시간에 배치된 Task와 아직 배치되지 않은 Task를 구분한다.
- Note는 **하루나 일정에 붙는 맥락**이다.
- Floe는 별도 dashboard를 만들지 않고 사용자의 시간축 위에 조용히 개입한다.

제품적으로는 다음과 같은 화면을 지향한다.

```text
┌───────────────────────────────────────────────────────────────┐
│  ‹     9월 3일 목요일        오늘        Day  Week           │
├───────────────────────────────────────────┬───────────────────┤
│                                           │                   │
│ all day     ○ 프로젝트 마감               │  오늘 할 일        │
│                                           │                   │
│  08 ───────────────────────────────────   │  □ PR 리뷰         │
│                                           │  □ 엄마 전화       │
│  09       ┌──────────────────────────┐    │                   │
│           │ 출근                     │    │  메모              │
│  10       └──────────────────────────┘    │                   │
│                                           │  Connector 구조    │
│  11 ───────────────────────────────────   │  다시 생각하기     │
│                                           │                   │
│           □ 메일 답장                      │  + 메모            │
│  12       ┌──────────────────────────┐    │                   │
│           │ 점심                     │    │                   │
│  13 ──────●──────────────────────────   │                   │
│           현재 시간                       │                   │
│  14       ┌──────────────────────────┐    │                   │
│           │ Design Review            │    │                   │
│  15       └──────────────────────────┘    │                   │
│                                           │                   │
│           ┌ ─ ─ ─ Floe ─ ─ ─ ─ ─ ┐      │                   │
│           │ 30분 휴식 제안          │      │                   │
│           └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘      │                   │
│                                           │                   │
├───────────────────────────────────────────┴───────────────────┤
│ +  무엇이든 적기...                                           │
└───────────────────────────────────────────────────────────────┘
```

위 다이어그램은 레이아웃 계약이 아니라 **정보 위계와 mental model**을 설명한다.

---

# 1. Primary Mental Model

사용자가 Day Canvas를 봤을 때 가장 먼저 이해해야 하는 것은:

> **오늘 내 시간이 어떻게 흘러가는가?**

이다.

그 다음에:

> 오늘 무엇을 해야 하는가?

> 기억해둘 것이 있는가?

> Floe가 지금 알려줄 것이 있는가?

가 따라온다.

따라서 Day Canvas의 hierarchy는 기본적으로 다음과 같다.

| 데이터 | 기본 위치 | 시각적 무게 |
| --- | --- | ---: |
| Event | Calendar time grid | 가장 강함 |
| Current time | Calendar time grid | 매우 명확 |
| Scheduled Task | Calendar time grid | 중간 |
| Unscheduled Today Task | Today rail / compact task surface | 낮음~중간 |
| Note | Today rail / Event annotation | 낮음 |
| Floe proposal | 관련 시간/항목의 ghost layer | 필요할 때만 |
| Health / Memory raw context | 기본 화면에 직접 노출하지 않음 | 숨김 |

**Domain equality does not imply visual equality.**

Event, Task, Note가 모두 중요한 domain object라고 해서 같은 row template과 같은 시각적 무게를 가져서는 안 된다.

---

# 2. Calendar Grid

Desktop Day Canvas의 주 surface는 Apple Calendar의 Day view처럼 즉시 이해할 수 있는 vertical time grid를 기본으로 한다.

필수 요소:

- 날짜 navigation
- all-day 영역
- hour / sub-hour grid
- current-time indicator
- timed Event block
- overlapping Event 처리
- scroll-to-now
- keyboard / pointer 기반 빠른 navigation

초기 제품은 **Day view를 primary home**으로 삼는다.

Week view는 자연스러운 확장이지만 Day Canvas의 첫 acceptance criteria는 아니다. Month view는 일정 탐색에는 유용하지만 Floe의 "오늘을 잘 운영한다"는 핵심 경험의 중심은 아니다.

## Event Rendering

Event는 실제 시간 간격에 비례해 공간을 차지한다.

```text
14:00 ┌──────────────────────────────┐
      │ Design Review               │
      │ 14:00–15:00                 │
15:00 └──────────────────────────────┘
```

Calendar source color가 존재하는 경우 full saturated card보다 다음을 선호한다.

```text
┌ blue accent  Design Review
│              14:00–15:00
```

즉:

- 2–3px source accent
- 매우 옅은 source tint 또는 neutral surface
- text/icon/label로도 의미 전달

을 기본으로 하여 Calm UI와 scanability를 동시에 유지한다.

## All-day Events

All-day Event는 `00:00–24:00` block처럼 렌더링하지 않는다.

Calendar grid 상단의 별도 all-day strip에 compact하게 표시한다.

---

# 3. Now / Next

`Now / Next`는 Floe에서 여전히 중요하지만 **큰 hero panel이 아니다.**

Now/Next는 calendar를 읽고 이동하기 쉽게 만드는 navigation intelligence다.

## Now

기본 Now 표현은 current-time line이다.

```text
13:24 ───────●────────────────────────
```

현재 진행 중인 Event가 있다면 해당 Event를 subtle하게 강조한다.

Day Canvas 상단에 동일한 정보를 반복하는 대형 `지금` card를 두지 않는다.

## Next

다음 일정은 calendar 자체에서 확인 가능해야 한다.

다음 Event가 viewport 밖에 있어 사용자가 즉시 볼 수 없는 경우에만 compact sticky affordance를 사용할 수 있다.

```text
다음  14:00 Design Review · 36분 후  ↓
```

이 affordance는 navigation aid이지 permanent dashboard section이 아니다.

---

# 4. Tasks

모든 Task를 calendar time grid에 강제로 넣지 않는다.

Floe는 Task의 두 상태를 시각적으로 구분한다.

## Scheduled Task

실제로 특정 시간에 수행하기로 계획한 Task.

```text
11:20  □ 메일 답장
```

Calendar grid에 Event보다 가벼운 형태로 나타난다.

중요: **Task deadline과 scheduled time은 다른 개념이다.**

향후 domain model에서 time allocation이 필요하다면 `deadline`을 scheduled time으로 재사용하지 않고 명시적인 scheduling semantics를 정의한다.

## Unscheduled Today Task

오늘 해야 하지만 정확한 시간은 정하지 않은 Task.

```text
오늘 할 일

□ PR 리뷰
□ 엄마 전화
□ 장보기
```

Desktop에서는 compact Today rail이 가장 유력한 표현이다.

사용자가 원하거나 Floe가 제안하면 unscheduled Task를 time grid로 배치할 수 있다.

```text
PR 리뷰
   ↓
Floe: 16:00이 비어 있음
   ↓
16:00 ghost placement
   ↓ accept
Scheduled Task
```

이 흐름은 Floe의 schedule intelligence가 사용자에게 자연스럽게 드러나는 핵심 UX다.

---

# 5. Notes

Note는 기본적으로 Calendar의 시간 object가 아니다.

Note는 다음 두 형태를 우선한다.

## Today Note

그날 기억하고 싶은 생각이나 짧은 기록.

Desktop Today rail 등 낮은 시각적 무게의 surface에서 보여준다.

```text
메모

Connector 구조 다시 보기
+ 메모
```

## Contextual Note

특정 Event, Task, Person 등에 연결된 Note.

예:

```text
14:00 Design Review
      API 변경안 이야기하기  ↗ note
```

시간축에 독립적인 Note row를 계속 추가해 Calendar readability를 깨지 않는다.

---

# 6. Floe Interventions

Floe는 Day Canvas 안에 별도의 `Insights`, `AI`, `Assistant` dashboard를 갖지 않는다.

제안은 **관련 시간 또는 object에 붙는다.**

예:

```text
15:00 ┌ ─ ─ ─ ─ Floe ─ ─ ─ ─ ┐
      │ 30분 정도 비워두는 게 │
      │ 좋아 보여요.           │
      └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘
```

또는 일정 이동 제안:

```text
18:00 Gym

18:40 Gym  ┄┄┄   Floe proposal
```

Intervention은 기본적으로 ghost / proposed state로 표현한다.

- Accept → canonical Timeline mutation
- Dismiss → 사라짐
- Details → 필요한 근거만 progressive disclosure

Health, Memory, Expert insight의 원시 데이터를 직접 calendar에 표시하지 않는다.

---

# 7. Today Rail

Desktop에서 Calendar 옆에 secondary surface를 둘 수 있다.

목적은 dashboard가 아니라 **시간축에 아직 배치되지 않은 오늘의 context**를 담는 것이다.

초기 후보:

```text
오늘 할 일
메모
```

원칙:

- Calendar보다 좁고 약하다.
- 고정 width 후보는 약 240–280px 수준이며 실제 PoC로 조정한다.
- statistics, streak, productivity score, Expert별 panel을 넣지 않는다.
- 내용이 없으면 빈 공간을 허용한다.
- 필요 없는 사용자는 rail을 접을 수 있어야 한다.

---

# 8. Responsive Behavior

## Desktop

```text
Calendar Grid — dominant / fluid
Today Rail    — compact / optional
```

Calendar가 항상 공간 우선권을 가진다.

## Narrow Desktop / Tablet

Today rail을 숨기거나 overlay / drawer로 전환한다.

Calendar width를 지나치게 줄여 hour grid를 읽기 어렵게 만들지 않는다.

## Mobile

모바일에서도 calendar/time mental model을 유지하되 desktop layout을 축소 복제하지 않는다.

후보:

- Day timeline이 primary
- Tasks / Notes는 bottom sheet 또는 secondary tab/sheet
- current time 주변으로 자동 positioning
- 빠른 capture는 thumb-reachable surface

`Experience parity, not composition parity` 원칙을 따른다.

---

# 9. Universal Capture in Day Canvas

Capture는 항상 쉽게 접근 가능해야 하지만 chat composer처럼 보여서는 안 된다.

장기 제품 target은:

```text
입력
 ↓
원문 안전하게 저장
 ↓
필요하면 Floe가 구조화 후보 생성
 ↓
사용자가 즉시 하던 일 계속
```

이다.

현재 Personal Day vertical slice의 explicit Event / Task / Note classification은 canonical data를 신뢰성 있게 만드는 초기 PoC 전략이다.

하지만 장기 UX에서 **모든 capture 직후 blocking classification dialog를 강제하지 않는다.**

사용자는:

- 즉시 분류
- Floe suggestion 수락
- 나중에 정리

중 하나를 선택할 수 있어야 한다.

Pending Capture가 사용자의 시야에서 유실되지 않는 recovery/review path가 필요하다.

---

# 10. Dense Days

Floe는 정보가 많다고 해서 모든 것을 같은 강도로 보여주지 않는다.

우선순위:

1. current time과 nearby Event readability
2. overlap이 있는 실제 Event
3. scheduled Task
4. 중요한 Floe proposal
5. secondary annotations

Dense day에서 고려할 수 있는 전략:

- zoom / hour-height adjustment
- overlap lane
- compact short-event representation
- completed Task collapse
- secondary note annotation fold
- offscreen next-event navigation

**기본 font와 touch target을 무작정 줄이는 방식은 마지막 수단이다.**

---

# 11. Internal Model vs Projection

UI가 Calendar-first라고 해서 domain을 Calendar object 하나로 합치지 않는다.

```text
Personal Timeline
├─ Event
├─ Task
├─ Note
├─ Commitment
└─ Intervention / Proposal
       ↓
Calendar-first Day Projection
```

Event의 recurrence/timezone/attendee와 Task의 deadline/completion/scheduling, Note의 content/context semantics를 유지한다.

Day projection은 각 object를 **어디에, 어떤 시각적 무게로 배치할지** 결정한다.

---

# 12. Non-goals

Day Canvas는 다음이 아니다.

- Now/Next KPI dashboard
- Event/Task/Note가 같은 row로 나열되는 generic feed
- AI insight dashboard
- Health dashboard
- productivity score board
- 모든 data source를 한 화면에 펼치는 control center

---

# 13. Open Questions

구현 전/동시에 dogfood로 결정해야 한다.

- Default hour height와 zoom range
- Scheduled Task의 정확한 domain semantics
- Today rail의 기본 open/closed 상태
- Note annotation과 Today Note의 연결 모델
- overlapping Event visual policy
- Floe ghost proposal의 accept/dismiss interaction
- Calendar source accent의 색상 강도
- mobile에서 Task/Note secondary surface의 정확한 형태
- multi-day / all-day Event 밀도가 높은 경우의 처리

## Acceptance Direction

새 Day Canvas가 성공하려면 사용자가 첫눈에:

1. **지금 몇 시이고 오늘 어떤 일정이 있는지** 알 수 있고,
2. **오늘 해야 할 일**을 별도 Todo 앱을 열지 않고 볼 수 있고,
3. **짧은 메모**를 별도 Notes 앱을 열지 않고 남길 수 있으며,
4. Floe의 제안은 필요할 때만 보이고,
5. 화면이 Calendar + Todo + Notes를 모두 담고 있음에도 산만하지 않아야 한다.
