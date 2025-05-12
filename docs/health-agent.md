# HealthAgent Specification

## Purpose

Aggregate physiological and lifestyle data, monitor wellbeing, and suggest restorative or preventive actions.

## Data Sources

* Apple HealthKit via local bridge
* Google Fit REST
* Manual logs from ConversationAgent

## Sub-module Agents

| Module                   | Focus                         | Key Metrics                 |
| ------------------------ | ----------------------------- | --------------------------- |
| **SleepModuleAgent**     | Sleep stages & recovery       | sleep\_score, sleep\_debt   |
| **ActivityModuleAgent**  | Workouts & movement           | step\_count, training\_load |
| **NutritionModuleAgent** | Meals & macros                | kcal\_intake, protein\_g    |
| **WellnessModuleAgent**  | Stress & subjective wellbeing | hrv, mood\_score            |

## Skills

| Skill             | Args                         | Returns      |
| ----------------- | ---------------------------- | ------------ |
| `log_metric`      | `type`, `value`, `timestamp` | `entryId`    |
| `detect_overload` | `window`                     | `status`     |
| `propose_break`   | `context`                    | `suggestion` |
| `aggregate_daily` | `date`                       | `summary`    |

## Wellness Rules

* Sleep debt > 90 min → notify ScheduleAgent to block recovery time.
* HRV drop > 20 % & high workload → suggest light day.
* Calorie deficit > 500 kcal 3 days consecutively → recommend nutrition plan.

## Privacy & Consent

* Explicit opt-in for each data source.
* Data stored encrypted; raw retention 30 days, aggregates forever.
