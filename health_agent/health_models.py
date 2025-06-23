from __future__ import annotations
import datetime
from typing import Optional
from pydantic import BaseModel, Field

class SleepRecord(BaseModel):
    user_id: str
    start_time_utc: datetime.datetime
    end_time_utc: datetime.datetime
    quality_score: Optional[float] = None
    source: str = Field(default="manual_entry")

class MealRecord(BaseModel):
    user_id: str
    timestamp_utc: datetime.datetime
    description: str
    calories: Optional[float] = None
    protein_g: Optional[float] = None
    carbs_g: Optional[float] = None
    fat_g: Optional[float] = None
    source: str = Field(default="manual_entry")

class ActivityRecord(BaseModel):
    user_id: str
    start_time_utc: datetime.datetime
    duration_minutes: float
    activity_type: str
    intensity: Optional[str] = None
    calories_burned: Optional[float] = None
    source: str = Field(default="manual_entry")

class WellnessLog(BaseModel):
    user_id: str
    timestamp_utc: datetime.datetime
    stress_level: Optional[int] = None
    mood: Optional[int] = None
    notes: Optional[str] = None
    source: str = Field(default="manual_entry")
