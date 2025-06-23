from __future__ import annotations
import datetime
from typing import List
from pydantic import BaseModel

from .health_models import SleepRecord
from .wearable_connectors import WearableConnector

class SleepMetrics(BaseModel):
    sleep_score: float
    sleep_debt: int  # minutes

class SleepModule:
    """Utilities for processing sleep data."""

    def __init__(self, connector: WearableConnector | None = None):
        self.connector = connector

    def log_sleep(self, user_id: str, sleep_data: SleepRecord) -> bool:
        # Placeholder for persistence logic (e.g., MemoryManager or MCP)
        # For now we simply pretend it was stored successfully
        return True

    def calculate_sleep_deficit(self, recent_hours: List[float], target_hours: float = 7.5) -> float:
        if not recent_hours:
            return 0.0
        avg = sum(recent_hours) / len(recent_hours)
        return max(0.0, target_hours - avg)

    def suggest_sleep_recovery(self, deficit_hours: float) -> str:
        if deficit_hours <= 0:
            return "You're meeting your sleep target."
        return f"Try to get an extra {deficit_hours:.1f} hours of sleep tonight."

    def analyze_sleep(self, session: SleepRecord) -> SleepMetrics:
        duration = (session.end_time_utc - session.start_time_utc).total_seconds() / 3600
        score = min(100.0, max(0.0, duration / 8 * 100))
        debt_minutes = int(max(0.0, (8 - duration) * 60))
        return SleepMetrics(sleep_score=score, sleep_debt=debt_minutes)

    def recommend_bedtime(self, next_day_start: datetime.datetime) -> datetime.datetime:
        # Simple heuristic: aim for 8 hours before next day's start
        return next_day_start - datetime.timedelta(hours=8)

    def import_wearable_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[SleepRecord]:
        if not self.connector:
            return []
        return self.connector.fetch_sleep_data(user_id, start, end)
