from __future__ import annotations
import datetime
from typing import List, Optional
from pydantic import BaseModel

from .health_models import MealRecord
from .wearable_connectors import WearableConnector

class MacroBreakdown(BaseModel):
    calories: float = 0.0
    protein_g: float = 0.0
    carbs_g: float = 0.0
    fat_g: float = 0.0

class NutritionModule:
    def __init__(self, connector: WearableConnector | None = None):
        self.connector = connector
        self.meal_logs: dict[str, List[MealRecord]] = {}

    def log_meal(self, user_id: str, meal_data: MealRecord) -> bool:
        logs = self.meal_logs.setdefault(user_id, [])
        logs.append(meal_data)
        return True

    def get_meals_for_date(self, user_id: str, date: datetime.date) -> List[MealRecord]:
        return [
            meal
            for meal in self.meal_logs.get(user_id, [])
            if meal.timestamp_utc.date() == date
        ]

    def track_daily_nutrients(self, meals: List[MealRecord]) -> MacroBreakdown:
        totals = MacroBreakdown()
        for meal in meals:
            if meal.calories:
                totals.calories += meal.calories
            if meal.protein_g:
                totals.protein_g += meal.protein_g
            if meal.carbs_g:
                totals.carbs_g += meal.carbs_g
            if meal.fat_g:
                totals.fat_g += meal.fat_g
        return totals

    def analyze_intake(self, meals: List[MealRecord]) -> MacroBreakdown:
        return self.track_daily_nutrients(meals)

    def suggest_meal(self, goal: str, dietary_restrictions: Optional[List[str]] = None) -> str:
        return "Consider a balanced meal with lean protein and vegetables."

    def import_activity_calories(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> float:
        if not self.connector:
            return 0.0
        activities = self.connector.fetch_activity_data(user_id, start, end)
        return sum(a.calories_burned or 0 for a in activities)
