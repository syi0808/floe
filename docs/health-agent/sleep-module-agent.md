# SleepModuleAgent Specification

## Purpose

Analyse nightly sleep stages, compute recovery scores, and surface insights to HealthAgent and InsightAgent.

## Inputs

* Sleep sessions (start, end, stage breakdown) from wearables.
* Subjective sleep quality rating (1‑5) from user.

## Outputs

* `sleep_score` (0‑100) weighted by duration, efficiency, REM/Deep %.
* `sleep_debt` minutes.

## Skills

| Skill               | Args              | Returns        |
| ------------------- | ----------------- | -------------- |
| `analyze_sleep`     | `session`         | `sleepMetrics` |
| `recommend_bedtime` | `nextDaySchedule` | `bedtime`      |

## Algorithms

* Two‑night rolling baseline.
* Exponential decay on older data.

## Notifications

* If sleep\_debt > 120 min  → Suggest earlier bedtime via ConversationAgent.
