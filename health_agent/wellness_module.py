from __future__ import annotations
import datetime
from typing import List, Optional
from pydantic import BaseModel

from .health_models import WellnessLog
from .wearable_connectors import WearableConnector

class StressAssessment(BaseModel):
    stress_level: str

class WellnessModule:
    def __init__(self, connector: WearableConnector | None = None):
        self.connector = connector

    def log_wellness_checkin(self, user_id: str, wellness_data: WellnessLog) -> bool:
        return True

    def analyze_stress_patterns(self, logs: List[WellnessLog]) -> Optional[str]:
        if not logs:
            return None
        avg = sum((log.stress_level or 0) for log in logs) / len(logs)
        if avg > 4:
            return "high"
        if avg > 2:
            return "medium"
        return "low"

    def recommend_recovery_routine(self, stress_level: int, available_time_minutes: int) -> str:
        if stress_level >= 4:
            return "Take a 10-minute breathing break and schedule focus time."
        return "Consider a short walk or stretching session."

    def import_wearable_hrv(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[float]:
        if not self.connector:
            return []
        # Wearable connectors might return ActivityRecord including HRV.
        activities = self.connector.fetch_activity_data(user_id, start, end)
        return [a.calories_burned or 0 for a in activities]
