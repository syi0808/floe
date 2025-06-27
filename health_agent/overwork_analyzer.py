from __future__ import annotations
import datetime
from typing import List, Dict, Any, Optional


def _total_hours(events: List[Dict[str, Any]]) -> float:
    return sum((e['end'] - e['start']).total_seconds() for e in events) / 3600


def check_overwork(
    user_id: str,
    schedule_data: List[Dict[str, Any]],
    task_data: List[Dict[str, Any]],
    recent_activity: List[Dict[str, Any]],
) -> Optional[str]:
    """Return warning string if user appears overworked."""
    hours_scheduled = _total_hours(schedule_data)
    high_priority_tasks = sum(1 for t in task_data if t.get('priority', 0) <= 1)
    steps = sum(a.get('steps', 0) for a in recent_activity)
    stress = sum(a.get('stress_level', 0) for a in recent_activity) / max(len(recent_activity), 1)
    if hours_scheduled > 10 or high_priority_tasks > 5 or stress > 4:
        return (
            "Your schedule and tasks indicate possible overwork. "
            "Consider taking short breaks and reviewing priorities."
        )
    if steps < 2000 and hours_scheduled > 8:
        return (
            "You have a long work day with little activity logged. "
            "Try to fit in a walk or stretch break."
        )
    return None
