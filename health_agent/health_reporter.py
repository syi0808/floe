from __future__ import annotations
import datetime
from typing import List

from .sleep_module import SleepModule
from .nutrition_module import NutritionModule
from .activity_module import ActivityModule
from .wellness_module import WellnessModule


class HealthReporter:
    def __init__(
        self,
        sleep: SleepModule,
        nutrition: NutritionModule,
        activity: ActivityModule,
        wellness: WellnessModule,
    ) -> None:
        self.sleep = sleep
        self.nutrition = nutrition
        self.activity = activity
        self.wellness = wellness

    def generate_weekly_summary(self, user_id: str) -> str:
        today = datetime.datetime.utcnow().date()
        week_start = today - datetime.timedelta(days=7)
        sleep_hours = [
            (record.end_time_utc - record.start_time_utc).total_seconds() / 3600
            for record in self.sleep.sleep_logs.get(user_id, [])
            if record.start_time_utc.date() >= week_start
        ]
        avg_sleep = sum(sleep_hours) / len(sleep_hours) if sleep_hours else 0.0

        meals = [
            r
            for r in self.nutrition.meal_logs.get(user_id, [])
            if r.timestamp_utc.date() >= week_start
        ]
        total_calories = sum(m.calories or 0 for m in meals)

        steps = sum(
            int(a.duration_minutes)
            for a in self.activity.activity_logs.get(user_id, [])
            if a.start_time_utc.date() >= week_start and a.activity_type == "steps"
        )

        stress_levels = [
            w.stress_level or 0
            for w in self.wellness.wellness_logs.get(user_id, [])
            if w.timestamp_utc.date() >= week_start
        ]
        avg_stress = sum(stress_levels) / len(stress_levels) if stress_levels else 0

        return (
            f"Avg sleep: {avg_sleep:.1f}h, "
            f"Total calories: {total_calories:.0f}, "
            f"Steps: {steps}, "
            f"Avg stress: {avg_stress:.1f}"
        )
