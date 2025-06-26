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
        self.wellness_logs: dict[str, List[WellnessLog]] = {}

    def log_wellness_checkin(self, user_id: str, wellness_data: WellnessLog) -> bool:
        logs = self.wellness_logs.setdefault(user_id, [])
        logs.append(wellness_data)
        return True

    def get_recent_logs(self, user_id: str, days: int = 7) -> List[WellnessLog]:
        cutoff = datetime.datetime.utcnow() - datetime.timedelta(days=days)
        return [
            w
            for w in self.wellness_logs.get(user_id, [])
            if w.timestamp_utc >= cutoff
        ]

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
        # TODO: Implement proper HRV data extraction from wearable devices
        # HRV data should come from heart rate measurements, not activity calories
        raise NotImplementedError("HRV data import requires proper implementation")
