from __future__ import annotations
from typing import List
import datetime
from .health_models import SleepRecord, ActivityRecord

class WearableConnector:
    """Base class for wearable device connectors."""

    def fetch_sleep_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[SleepRecord]:
        raise NotImplementedError

    def fetch_activity_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[ActivityRecord]:
        raise NotImplementedError

    def fetch_hrv_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[float]:
        """Return heart-rate variability (HRV) measurements between start and end."""
        raise NotImplementedError

class GarminConnector(WearableConnector):
    """Example Garmin connector."""

    def fetch_sleep_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[SleepRecord]:
        # Placeholder implementation - integrate Garmin API here
        return []

    def fetch_activity_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[ActivityRecord]:
        return []

    def fetch_hrv_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[float]:
        # Placeholder - integrate Garmin HRV API
        return []

class FitbitConnector(WearableConnector):
    """Example Fitbit connector."""

    def fetch_sleep_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[SleepRecord]:
        return []

    def fetch_activity_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[ActivityRecord]:
        return []

    def fetch_hrv_data(self, user_id: str, start: datetime.datetime, end: datetime.datetime) -> List[float]:
        # Placeholder - integrate Fitbit HRV API
        return []
