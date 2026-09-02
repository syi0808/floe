# Personal Memory

> Status: Core domain, high-risk

## 정의

Floe Memory는 단순 preference store가 아니다.

**사용자의 개인사와 인간관계, 과거 경험, 약속을 장기적으로 기억하는 Personal Context System**이다.

## 주요 Memory Type

```text
User-confirmed Fact
Observation
Inference
Preference
Episode
Commitment
```

## Evidence First

```text
Immutable Evidence
├─ conversation
├─ transcript
├─ email
├─ calendar
└─ note
       ↓
Claims / Episodes
       ↓
Current Memory View
```

Summary를 반복해서 다시 요약하는 방식으로 원본을 소실하지 않는다.

Summary는 materialized view에 가깝게 취급한다.

## Temporal Memory

가능한 경우 다음을 유지한다.

```text
value
validFrom
validUntil
observedAt
confidence
source
```

"요즘 이직 준비 중"을 영구 사람 속성으로 만들지 않는다.

## Fact vs Inference

예:

```text
Observation:
최근 아버지와의 대화에서 회사 이야기가 자주 등장했다.

Inference:
아버지가 회사 일로 스트레스를 받고 있을 가능성이 있다.
```

Inference를 Fact로 승격하지 않는다.

## Memory Compilation

```text
Conversation / Email / Transcript
       ↓
Memory Candidates
       ↓
Structured Extraction
       ↓
Entity Resolution
       ↓
Existing Memory Comparison
       ↓
Memory Policy
       ↓
Long-term Memory
```

## Memory Safety

외부 문서/이메일은 항상 untrusted data로 취급한다.

외부 content가 prompt instruction으로 Memory에 들어가서는 안 된다.

typed claim 중심으로 저장하는 방향을 선호한다.

## 사용자 통제

Memory는:

- inspectable
- editable
- deletable
- traceable

이어야 한다.

## Forgetting

"전 여자친구 관련 기억을 모두 삭제" 같은 요청은 Person row 하나 삭제로 끝나지 않는다.

관련 episode, summary, embedding, cache, derived context를 추적할 provenance graph가 필요할 수 있다.
