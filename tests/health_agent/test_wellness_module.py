import datetime
from health_agent.wellness_module import WellnessModule, WellnessLog
from health_agent.wearable_connectors import WearableConnector


def test_log_and_assess():
    mod = WellnessModule()
    now = datetime.datetime.utcnow()
    log = WellnessLog(user_id="u", timestamp_utc=now, stress_level=5)
    mod.log_wellness_checkin("u", log)
    logs = mod.get_recent_logs("u", days=1)
    level = mod.analyze_stress_patterns(logs)
    assert level == "high"


class DummyConnector(WearableConnector):
    def fetch_sleep_data(self, user_id, start, end):
        return []

    def fetch_activity_data(self, user_id, start, end):
        return []

    def fetch_hrv_data(self, user_id, start, end):
        return [45.2, 46.1]


def test_import_wearable_hrv():
    mod = WellnessModule(connector=DummyConnector())
    start = datetime.datetime.utcnow() - datetime.timedelta(days=1)
    end = datetime.datetime.utcnow()
    data = mod.import_wearable_hrv("u", start, end)
    assert data == [45.2, 46.1]


def test_import_wearable_hrv_no_connector():
    mod = WellnessModule()
    data = mod.import_wearable_hrv("u", datetime.datetime.utcnow() - datetime.timedelta(days=1), datetime.datetime.utcnow())
    assert data == []
