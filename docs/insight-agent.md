# InsightAgent Specification

## Purpose

Generate cross‑domain analytics and digest reports that help the user understand productivity, wellbeing, and trends over time.

## Data Inputs

* Aggregated event/task stats from Schedule & Task agents.
* MemoryManager aggregated counts (tasks completed, postponed).
* HealthAgent KPIs (sleep\_score, activity load).

## Skills

| Skill             | Args                   | Returns    |
| ----------------- | ---------------------- | ---------- |
| `generate_report` | `period`, `focus?`     | `markdown` |
| `generate_daily_report` | `focus?` | `markdown` |
| `generate_weekly_report` | `focus?` | `markdown` |
| `compare_period`  | `metric`, `from`, `to` | `diff`     |
| `goal_progress`   | `goalId`               | `status`   |

## Report Templates

* **Daily Brief** – agenda, priority tasks, recovery tip.
* **Weekly Review** – successes, slips, recommended focus.

## Visualisation

* Uses recharts via front‑end panel; InsightAgent returns JSON spec for client rendering.

## KPIs

* Digest generation < 2 s.
* 70 % of suggested focus areas accepted by user.

## Example Call

```jsonc
{
  "agent": "InsightAgent",
  "skill": "generate_report",
  "args": { "period": "2025‑W20" }
}
```

For convenience, you can also call dedicated helpers:

```python
gen = InsightGenerator()
gen.generate_daily_report(user_id, data, mcp_client=client)
gen.generate_weekly_report(user_id, data, mcp_client=client)
```
