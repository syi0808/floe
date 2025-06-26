import datetime
from health_agent.activity_module import ActivityModule, ActivityRecord


def test_log_and_analyze():
    mod = ActivityModule()
    now = datetime.datetime.utcnow()
    act = ActivityRecord(user_id="u", start_time_utc=now, duration_minutes=1000, activity_type="steps")
    mod.log_activity("u", act)
    recent = mod.get_recent_activity("u", days=1)
    metrics = mod.analyze_activity(recent)
    assert metrics.total_steps == 1000
