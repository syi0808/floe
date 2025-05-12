# NutritionModuleAgent Specification

## Purpose

Track meals and nutrient intake, assess diet quality, and provide personalised nutrition reminders or adjustments.

## Data Inputs

* Logged meals (photo, text, or barcode scan).
* Wearable-derived calorie expenditure for balance calculation.

## Skills

| Skill            | Args                                     | Returns           |
| ---------------- | ---------------------------------------- | ----------------- |
| `log_meal`       | `description`, `timestamp`, `nutrients?` | `mealId`          |
| `analyze_intake` | `window`                                 | `macro_breakdown` |
| `suggest_meal`   | `goal`, `dietary_restrictions?`          | `meal_plan`       |

## Algorithms

* USDA FoodData lookup for macro estimation.
* Rolling 7-day average compared against user goal (maintenance, bulk, cut).

## Notifications

* Protein intake below target 2 days → prompt high-protein recipe.
* 3-hour fasting window exceeded during day → gentle meal reminder.

## Privacy

* Images analysed locally; no cloud vision upload.
* Food logs encrypted at rest.
