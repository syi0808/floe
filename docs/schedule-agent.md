# ScheduleAgent Specification

## Purpose

Manage user calendars: create, update, and delete events; recommend optimal times; detect conflicts; and generate concise schedule summaries across multiple calendar sources.

## Integrations

* Google Calendar API (OAuth, push notifications)
* Microsoft Graph Calendar
* Local iCal (.ics) import/export

## Skills

| Skill                | Args                                                                      | Returns            |
| -------------------- | ------------------------------------------------------------------------- | ------------------ |
| `create_event`       | `title`, `start`, `end`, `attendees?`, `location?`                        | `eventId`          |
| `update_event`       | `eventId`, `patch`                                                        | `event`            |
| `suggest_time`       | `duration`, `window`, `preferences?`                                      | `slots[]`          |
| `detect_conflict`    | `start`, `end`                                                            | `conflicts[]`      |
| `summarize_schedule` | `period` (`"today"` \| ISO date range), `granularity?` (`daily`/`weekly`) | `markdown` summary |

## Time-Suggestion Algorithm

1. Merge busy blocks from all linked calendars.
2. Apply user preferences (working hours, focus blocks).
3. Score candidate slots by recency, contextual importance, and meeting density.

## Schedule Summary Generation

1. Collect events within `period`.
2. Bucket by day, then by priority (high-importance events first).
3. Output Markdown including free-time windows ≥ 1 h.

## Performance Targets

* Return ≤ 20 slot suggestions in < 400 ms.
* Generate daily summary in < 300 ms.
* Webhook latency < 5 s for external changes.

## Edge Cases

* Time-zone shifts → auto-recalculate on location change.
* All-day vs. multi-day events handled separately.

## Example Calls

```jsonc
// Suggest a 45-minute slot
{
  "agent": "ScheduleAgent",
  "skill": "suggest_time",
  "args": { "duration": "PT45M", "window": "2025-05-14/2025-05-18" }
}

// Get today’s overview
{
  "agent": "ScheduleAgent",
  "skill": "summarize_schedule",
  "args": { "period": "today" }
}
```
