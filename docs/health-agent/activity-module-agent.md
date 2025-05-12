# ActivityModuleAgent Specification

## Purpose

Track workouts and daily movement, detect imbalance, and propose exercise routines or rest.

## Data Inputs

* Steps, active calories, heart‑rate zones.
* Workout metadata (type, duration, intensity).

## Skills

| Skill              | Args                                | Returns     |
| ------------------ | ----------------------------------- | ----------- |
| `log_workout`      | `activity`, `duration`, `intensity` | `workoutId` |
| `analyze_activity` | `window`                            | `metrics`   |
| `suggest_exercise` | `goal`, `constraints?`              | `plan`      |

## Imbalance Detection Rules

* < 4,000 steps average 3 days → prompt light walk.
* Over 3 HiIT sessions/week without rest → suggest recovery.

## Example Output

```jsonc
{
  "weekly_summary": {
    "total_steps": 52000,
    "vo2max_trend": "+1.2",
    "recovery_days": 1
  }
}
```
