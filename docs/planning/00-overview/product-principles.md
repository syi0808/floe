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

### 판단에 필요한 정보만 먼저

2026-09-06 사용자 피드백: 액션 리뷰는 내부 실행 기록을 설명하는 화면이 아니라,
사용자가 Floe의 계획을 이해하고 결정하는 화면이다. 추상화는 정보를 단순히 숨기는
것이 아니라, 그 결정에 필요한 의미를 선별해 사람의 언어로 전달하는 일이다.

- 기본 리뷰는 **무엇을, 언제, 어디에 하며, 승인하면 무엇이 달라지는가**에 답한다.
- 사용자 제목, 실제 대상 캘린더/계정, 날짜·시간은 보존한다. 시간 데이터는 UTC로
  저장하되 입력과 출력은 모두 사용자의 현재 로컬 시간으로 처리한다. 시간대 이름,
  UTC 표기와 offset은 사용자의 판단 정보로 노출하지 않는다.
- UUID, 내부 provider 코드, 실행/제안 ID, 고정 Person ID는
  기본 리뷰에 나열하지 않는다. 감사·지원에 필요한 원본은 명시적으로 펼치는
  진단 상세에 보존한다. 정보의 추상화가 실행 기록의 삭제를 뜻하지 않는다.
- 읽는 사람이 승인 여부나 다음 행동을 바꾸게 되는 정보만 기본 화면에 남긴다.
  반복되는 상태 설명, 내부 처리 단계, 구현 용어는 이 기준으로 걷어낸다.
- 초대/알림/공유/삭제 등 실제 영향, 권한 제한, 만료·충돌, 결과 불확실성은
  시각적 단순함을 위해 숨기지 않는다. 관련된 상태에서 짧고 구체적으로 보여준다.
- 외부 생성 성공과 앱 재수집 성공을 혼동하지 않는다. 불확실한 결과에는
  재생성이 아니라 조회를, 수집 실패에는 읽기 재시도를 안내한다.
- 승인 버튼은 수행할 행동을 말해야 하며, 간결한 표현 때문에 승인 범위가
  넓어지거나 자동 실행으로 바뀌어서는 안 된다.

이 원칙은 제안뿐 아니라 연결 상태, 알림, 오류 복구에도 적용한다. 새 정보 항목을
추가할 때는 “이 정보가 지금 사용자의 어떤 판단을 돕는가?”를 먼저 확인한다.

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

## 13. Extensible Expertise, Stable Experience

Users can extend what Floe knows how to monitor and advise on without turning Floe into a collection of competing apps.

Third-party Experts integrate through stable data/action contracts while the Manager and Calm UI remain in control of the user experience.
