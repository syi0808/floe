# Universal Capture & Transcription

> Status: Core UX

## Universal Capture

사용자는 먼저 데이터 타입을 고르지 않는다.

```text
[ 무엇이든 입력... ]
```

예:

```text
"내일 3시 치과"
→ Event

"우유 사기"
→ Task

"Connector 구조 다시 보기"
→ Note

"다음 주 금요일 엄마 병원 같이 가기로 했어"
→ Commitment + Memory Candidate
```

## 음성도 동일 pipeline

```text
Voice/Text
   ↓
Capture
   ↓
Semantic interpretation
   ↓
Domain candidates
   ↓
Timeline / Memory / Action
```

## Transcription의 목적

Transcript 자체가 최종 제품이 아니다.

```text
Transcript
├─ Summary
├─ People
├─ Decisions
├─ Tasks
├─ Commitments
├─ Events
└─ Memory Candidates
```

예:

```text
"금요일까지 프로토타입 부탁드릴게요."
```

→

```text
Friday
□ 프로토타입 완성
source: Design Meeting · 13:42
```

## 신뢰성 경계

STT 결과를 곧바로 장기 Memory로 승격하지 않는다.

```text
Audio
 ↓
Transcript
 ↓
Claim extraction
 ↓
Entity resolution
 ↓
Confidence
 ↓
Memory Candidate
 ↓
Memory policy
```

사람 이름, 날짜, 금액, 약속은 잘못 추출될 때 영향이 크므로 별도 confidence 정책이 필요하다.

## Provenance

추출된 Task/Commitment/Memory는 가능하면 원본 transcript 위치로 돌아갈 수 있어야 한다.
