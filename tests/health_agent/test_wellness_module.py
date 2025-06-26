import datetime
from health_agent.wellness_module import WellnessModule, WellnessLog


def test_log_and_assess():
    mod = WellnessModule()
    now = datetime.datetime.utcnow()
    log = WellnessLog(user_id="u", timestamp_utc=now, stress_level=5)
    mod.log_wellness_checkin("u", log)
    logs = mod.get_recent_logs("u", days=1)
    level = mod.analyze_stress_patterns(logs)
    assert level == "high"
