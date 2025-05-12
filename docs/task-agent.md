# TaskAgent Specification

## Purpose

Convert user intents and extracted items into actionable tasks, manage their lifecycle, and synchronise with ScheduleAgent for time‑blocking.

## Skills

| Skill            | Args                            | Returns   |
| ---------------- | ------------------------------- | --------- |
| `add_task`       | `title`, `due?`, `priority?`    | `taskId`  |
| `update_task`    | `taskId`, `patch`               | `task`    |
| `schedule_block` | `taskId`, `duration`, `window?` | `eventId` |
| `snooze`         | `taskId`, `until`               | `task`    |

## Prioritisation Formula

```
score = (importance * 0.5) + (urgency * 0.3) + (effort_inverted * 0.2)
```

## Data Model

* Tasks stored in SQLite, linked to events via `calendar_event_id`.
* Supports subtasks & tags.

## Example Flow

1. InboxAgent detects phrase "Please review by Friday" → TaskAgent `add_task`.
2. TaskAgent `schedule_block` 90 min before due.

## KPIs

* ≥ 90 % of tasks have due date.
* Snoozed tasks resurfaced on time.
