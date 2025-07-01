# ConversationAgent Specification

## Purpose

Serve as the single natural‑language interface: collects user inputs (text/voice), maintains dialogue context, and mediates clarifications.

## Dialogue Stack

1. **Input Normalisation** – ASR → text, language detection.
2. **Context Assembly** – last 5 user turns + relevant memories.
3. **LLM Generation** – system + user + tool calls.
4. **Response Delivery** – text, optional TTS.

## Skills

| Skill               | Args                  | Returns         |
| ------------------- | --------------------- | --------------- |
| `converse`          | `message`, `context?` | `reply`         |
| `ask_clarification` | `question`            | `user_response` |
| `handoff`           | `plan`                | `status`        |

## Clarification Policy

* Trigger if confidence < 0.6 or missing required slot.
* Use concise polar questions first; escalate to open‑ended if still ambiguous.

## Tone & Style

* Friendly professional; avoids jargon.
* Reflects user’s language preference.

## Latency Budget

* 300 ms local inference (excluding ASR/TTS).

## Sample Flow

User: "Schedule code review tomorrow afternoon"
→ ConversationAgent ➜ Orchestrator ➜ ScheduleAgent ➜ ConversationAgent ✓

## History Persistence

ConversationAgent can persist dialogue history through `MemoryManagerAgent`.
Two helper methods are available:

```python
agent.load_history_from_memory(user_id)
agent.store_last_turn_to_memory(user_id)
```

These load past turns when a new instance starts and store the latest turn
after generating a response.
