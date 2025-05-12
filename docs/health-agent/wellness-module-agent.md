# WellnessModuleAgent Specification

## Purpose

Monitor stress, mood, and overall wellbeing to detect early signs of burnout and recommend recovery or mindfulness activities.

## Data Inputs

* Heart‑rate variability (HRV) trend from wearables.
* Self‑reported mood check‑ins (1–5 scale).
* Workload data (task count, meeting hours) from Task & Schedule agents.

## Skills

| Skill                | Args                           | Returns                       |
| -------------------- | ------------------------------ | ----------------------------- |
| `log_mood`           | `rating`, `timestamp`, `note?` | `entryId`                     |
| `assess_stress`      | `window`                       | `stress_level` (low/med/high) |
| `recommend_recovery` | `context`                      | `activity`                    |

## Stress Assessment Rules

```
stress_score = (workload_z + hrv_z_inv + mood_inv) / 3
```

* High if > 0.7; Medium 0.4–0.7; Low < 0.4.

## Recovery Suggestions

| Trigger                        | Recommendation                                 |
| ------------------------------ | ---------------------------------------------- |
| High stress                    | 10‑min breathing exercise + block "focus time" |
| Consecutive high stress 3 days | Suggest day off & light exercise               |

## Privacy

* Mood logs encrypted; user can set auto‑delete after N days.
