from __future__ import annotations
import datetime
from typing import List, Optional
from pydantic import BaseModel

from .health_models import ActivityRecord
from .wearable_connectors import WearableConnector

class ActivityMetrics(BaseModel):
    total_steps: int = 0
    vo2max_trend: Optional[float] = None
    recovery_days: int = 0

class ActivityModule:
    def __init__(self, connector: WearableConnector | None = None):
        self.connector = connector

    def log_activity(self, user_id: str, activity_data: ActivityRecord) -> bool:
        return True

    def analyze_activity(self, activities: List[ActivityRecord]) -> ActivityMetrics:
        metrics = ActivityMetrics()
        for act in activities:
            if act.type == "steps":
                metrics.total_steps += int(act.duration_minutes)
        return metrics

    def suggest_exercise_routine(self, goal: str, constraints: Optional[List[str]] = None) -> str:
        return "Try a 30-minute moderate intensity workout today."

    def import_wearable_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[ActivityRecord]:
        if not self.connector:
            return []
        return self.connector.fetch_activity_data(user_id, start, end)
