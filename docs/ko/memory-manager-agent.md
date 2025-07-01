
# MemoryManagerAgent 명세

## 목적

사용자별 작업, 대화, 건강 기록 등 다양한 메모리를 저장하고 검색하며 정리합니다.

## 메모리 종류

| 유형 | TTL | 벡터화 | 암호화 |
| --- | --- | --- | --- |
| 단기 대화 | 7일 | ✅ | ✅ |
| 작업 및 일정 | 영구 | ✅ (제목+노트) | ✅ |
| 건강 로그 | 원본 30일, 집계는 영구 | ✅ | ✅ |

## 스킬

| 스킬 | 인자 | 반환 |
| --- | --- | --- |
| `store` | `memory: {type, payload}` | `memoryId` |
| `recall` | `query`, `limit` | `memories[]` |
| `forget` | `memoryId` | `success` |
| `search` | `embedding`, `k` | `memories[]` |

## 저장 엔진

* 사용자별 SQLite DB와 Chroma 벡터 인덱스 사용
* 행 단위 AES‑GCM 암호화

## 가비지 컬렉션

* 매일 실행하여 TTL과 LRU 적용
* 정리된 내역은 InsightAgent에 이벤트로 전달

## ConversationAgent와 함께 사용하기

대화 기록은 `conversation_turn` 형식으로 저장됩니다.
`ConversationAgent`의 `load_history_from_memory`와
`store_last_turn_to_memory` 메서드를 이용해 연동할 수 있습니다.

