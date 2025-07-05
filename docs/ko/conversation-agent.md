# ConversationAgent 명세

## 목적

사용자의 입력을 수집하고 대화 맥락을 유지하며 필요 시 명확화를 중재하는 자연어 인터페이스 역할을 합니다.

## 대화 흐름

1. **입력 정규화** – 음성→텍스트, 언어 감지
2. **맥락 조립** – 최근 5회 사용자 발화 + 관련 메모리
3. **LLM 생성** – 시스템 + 사용자 + 도구 호출
4. **응답 전달** – 텍스트, 선택적 TTS

## 스킬

| 스킬 | 인자 | 반환 |
| --- | --- | --- |
| `converse` | `message`, `context?` | `reply` |
| `ask_clarification` | `question` | `user_response` |
| `handoff` | `plan` | `status` |

## 명확화 정책

* 신뢰도 < 0.6 이거나 필수 슬롯이 없으면 트리거
* 간단한 예/아니오 질문부터 사용, 계속 모호하면 열린 질문 사용

## 톤과 스타일

* 친근하지만 전문적인 어조
* 사용자의 언어 선호 반영

## 지연 시간 목표

* ASR/TTS 제외 300 ms 이하

## 예시 흐름

사용자: "내일 오후 코드 리뷰 일정 잡아줘"
→ ConversationAgent ➜ Orchestrator ➜ ScheduleAgent ➜ ConversationAgent ✓

## 기록 보존

ConversationAgent는 `MemoryManagerAgent`를 통해 대화 기록을 저장할 수 있습니다.
사용 가능한 두 가지 헬퍼 메서드는 다음과 같습니다.

```python
agent.load_history_from_memory(user_id)
agent.store_last_turn_to_memory(user_id)
```

새 인스턴스가 시작될 때 이전 턴을 불러오고, 응답 생성 후 마지막 턴을 저장합니다.

`ConversationAgentWrapper`는 메모리 매니저와 `user_id`가 주어지면
초기화 시 자동으로 `load_history_from_memory`를 호출합니다.

