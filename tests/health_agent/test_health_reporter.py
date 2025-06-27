import datetime
from health_agent.health_reporter import HealthReporter
from health_agent.sleep_module import SleepModule, SleepRecord
from health_agent.nutrition_module import NutritionModule, MealRecord
from health_agent.activity_module import ActivityModule, ActivityRecord
from health_agent.wellness_module import WellnessModule, WellnessLog


def test_generate_weekly_summary():
    s = SleepModule()
    n = NutritionModule()
    a = ActivityModule()
    w = WellnessModule()
    reporter = HealthReporter(s, n, a, w)

    now = datetime.datetime.utcnow()
    s.log_sleep("u", SleepRecord(user_id="u", start_time_utc=now - datetime.timedelta(hours=8), end_time_utc=now))
    n.log_meal("u", MealRecord(user_id="u", timestamp_utc=now, description="meal", calories=500))
    a.log_activity("u", ActivityRecord(user_id="u", start_time_utc=now, duration_minutes=3000, activity_type="steps"))
    w.log_wellness_checkin("u", WellnessLog(user_id="u", timestamp_utc=now, stress_level=3))

    summary = reporter.generate_weekly_summary("u")
    assert "Avg sleep" in summary
