# InboxAgent Specification

## Purpose

Parse emails/notifications, extract actionable data, and delegate to appropriate agents (Schedule, Task, Memory).

## Supported Channels

* Gmail API (REST + Pub/Sub)
* Generic IMAP
* Slack incoming webhooks (future)

## Skills

| Skill              | Args                    | Returns     |
| ------------------ | ----------------------- | ----------- |
| `fetch_email`      | `query`, `limit`        | `threads[]` |
| `summarize_thread` | `threadId`              | `summary`   |
| `watch_thread`     | `threadId`, `condition` | `watchId`   |

## Extraction Heuristics

* Date detection via Chrono.
* Action verbs (“review”, “approve”, “schedule”) → TaskAgent.
* RSVP invites → ScheduleAgent.

## Privacy Controls

* OAuth scopes limited to read‑only unless sending is required.
* PII redacted before storing in MemoryManager.

## Example

Email subject: "Proposal review by 5/20" → TaskAgent `add_task` "Review proposal" due 2025‑05‑20.
