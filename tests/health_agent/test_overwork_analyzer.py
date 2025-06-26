import datetime
from health_agent.overwork_analyzer import check_overwork


def test_check_overwork():
    now = datetime.datetime.utcnow()
    schedule = [
        {'start': now, 'end': now + datetime.timedelta(hours=11)}
    ]
    tasks = [{'priority': 1} for _ in range(6)]
    activity = [{'steps': 5000, 'stress_level': 5}]
    msg = check_overwork('u', schedule, tasks, activity)
    assert msg is not None
