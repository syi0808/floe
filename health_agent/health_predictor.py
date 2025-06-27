from __future__ import annotations
import datetime
from typing import Optional, List, Dict, Any


def _find_free_block(events: List[Dict[str, Any]], start: datetime.datetime, end: datetime.datetime) -> Optional[tuple[datetime.datetime, datetime.datetime]]:
    """Return first free time block between start and end."""
    current = start
    for event in sorted(events, key=lambda e: e['start']):
        ev_start = event['start']
        ev_end = event['end']
        if ev_start > current and ev_start - current >= datetime.timedelta(hours=1):
            return current, ev_start
        current = max(current, ev_end)
    if end - current >= datetime.timedelta(hours=1):
        return current, end
    return None


def predict_sleep_times(user_id: str, schedule_agent_client) -> Optional[Dict[str, str]]:
    """Predict a reasonable sleep window for the user based on schedule gaps."""
    now = datetime.datetime.utcnow()
    tonight = datetime.datetime.combine(now.date(), datetime.time(21, 0))
    tomorrow_morning = tonight + datetime.timedelta(hours=10)
    events = schedule_agent_client.get_events(user_id, tonight, tomorrow_morning)
    free_block = _find_free_block(events, tonight, tomorrow_morning)
    if not free_block:
        return None
    sleep_start, sleep_end = free_block
    if (sleep_end - sleep_start).total_seconds() < 6 * 3600:
        return None
    predicted_wake = min(sleep_start + datetime.timedelta(hours=8), sleep_end)
    return {
        "predicted_sleep_start": sleep_start.isoformat(),
        "predicted_wake_up": predicted_wake.isoformat(),
    }


def predict_meal_times(user_id: str, schedule_agent_client) -> List[Dict[str, str]]:
    """Predict typical meal times considering the user's schedule."""
    now = datetime.datetime.utcnow()
    today_start = datetime.datetime.combine(now.date(), datetime.time(6, 0))
    today_end = today_start + datetime.timedelta(hours=18)
    events = schedule_agent_client.get_events(user_id, today_start, today_end)
    meal_windows = [
        ("breakfast", datetime.time(7, 0), datetime.time(9, 0)),
        ("lunch", datetime.time(12, 0), datetime.time(14, 0)),
        ("dinner", datetime.time(18, 0), datetime.time(20, 0)),
    ]
    predictions = []
    for meal_type, start_t, end_t in meal_windows:
        window_start = datetime.datetime.combine(now.date(), start_t)
        window_end = datetime.datetime.combine(now.date(), end_t)
        block = _find_free_block(events, window_start, window_end)
        if block:
            predictions.append({"meal_type": meal_type, "predicted_time": block[0].isoformat()})
    return predictions
