# MemoryManagerAgent Specification

## Purpose

Store, retrieve, and prune user‑specific memories spanning tasks, chats, health logs, and external files for contextual reasoning.

## Memory Types

| Type                | TTL                            | Vectorised?       | Encryption |
| ------------------- | ------------------------------ | ----------------- | ---------- |
| Short‑term dialogue | 7 days                         | ✅                 | ✅          |
| Tasks & events      | Forever                        | ✅ (title + notes) | ✅          |
| Health logs         | 30 days raw, aggregate forever | ✅                 | ✅          |

## Skills

| Skill    | Args                      | Returns      |
| -------- | ------------------------- | ------------ |
| `store`  | `memory: {type, payload}` | `memoryId`   |
| `recall` | `query`, `limit`          | `memories[]` |
| `forget` | `memoryId`                | `success`    |
| `search` | `embedding`, `k`          | `memories[]` |

## Storage Engine

* SQLite DB per user with Chroma vector index.
* Row‑level AES‑GCM encryption using device key.

## Garbage Collection

* Runs nightly; applies TTL & LRU.
* Emits `memory_pruned` event for InsightAgent.

## Example Interaction

```jsonc
{
  "agent": "MemoryManagerAgent",
  "skill": "recall",
  "args": { "query": "conference call notes", "limit": 3 }
}
```

## Privacy Guarantees

* No cloud sync unless user enables end‑to‑end encrypted backup.
* Forget skill physically deletes row + vector.
