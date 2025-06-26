import datetime
from health_agent.health_predictor import predict_sleep_times, predict_meal_times


class DummyScheduleClient:
    def __init__(self, events=None):
        self.events = events or []

    def get_events(self, user_id, start, end):
        return [e for e in self.events if e['start'] < end and e['end'] > start]


def test_predict_sleep_times():
    now = datetime.datetime.utcnow().replace(hour=22, minute=0, second=0, microsecond=0)
    events = []
    client = DummyScheduleClient(events)
    result = predict_sleep_times('u', client)
    assert result is not None
    assert 'predicted_sleep_start' in result


def test_predict_meal_times():
    now = datetime.datetime.utcnow().replace(hour=12, minute=30, second=0, microsecond=0)
    events = [
        {'start': now, 'end': now + datetime.timedelta(hours=1)}
    ]
    client = DummyScheduleClient(events)
    meals = predict_meal_times('u', client)
    assert any(m['meal_type'] == 'breakfast' for m in meals) or any(m['meal_type'] == 'dinner' for m in meals) or any(m['meal_type'] == 'lunch' for m in meals)
