import datetime
from health_agent.sleep_module import SleepModule, SleepRecord


def test_log_and_deficit():
    module = SleepModule()
    now = datetime.datetime.utcnow()
    rec1 = SleepRecord(user_id="u", start_time_utc=now - datetime.timedelta(hours=8), end_time_utc=now)
    module.log_sleep("u", rec1)
    hours = module.get_recent_sleep_hours("u", days=1)
    assert hours == [8.0]
    deficit = module.calculate_sleep_deficit(hours, target_hours=8.0)
    assert deficit == 0.0
