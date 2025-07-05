import pytest
from insight_agent.insight_generator import InsightGenerator


def sample_data():
    return {
        "schedule_agent": [
            {"id": 1, "start": "2025-06-01T09:00:00Z", "end": "2025-06-01T10:00:00Z"},
            {"id": 2, "duration_hours": 1.5},
        ],
        "task_agent": [
            {"id": "t1", "status": "todo"},
            {"id": "t2", "status": "done"},
        ],
        "health_agent": [
            {"sleep_score": 80.0},
            {"sleep_score": 70.0},
        ],
        "goals": [
            {"id": "g1", "target": 10, "current": 5},
            {"id": "g2", "target": 3, "current": 3},
        ],
        "trend_data": {
            "tasks_completed": [1, 2, 4],
            "sleep_score": [70, 75, 80],
        },
    }


def test_compile_aggregates():
    gen = InsightGenerator()
    summary = gen.compile(sample_data())

    assert summary["schedule_agent"]["count"] == 2
    assert summary["schedule_agent"]["total_hours"] == 2.5
    assert summary["schedule_agent"]["avg_event_hours"] == 1.25
    assert summary["task_agent"]["count"] == 2
    assert summary["task_agent"]["completed"] == 1
    assert summary["task_agent"]["completion_rate"] == 0.5
    assert summary["health_agent"]["count"] == 2
    assert summary["health_agent"]["avg_sleep_score"] == 75.0
    assert "schedule_hours" in summary["trends"]


def test_generate_summary_markdown_multi():
    gen = InsightGenerator()
    md = gen.generate_summary(sample_data(), format="markdown")
    assert "# Insight Report" in md
    assert "**schedule_agent**" in md
    assert "2 entries" in md
    assert "completed" in md
    assert "avg sleep score" in md
    assert "## Goal Progress" in md
    assert "g1" in md
    assert "## Trends" in md


def test_generate_summary_json_multi():
    gen = InsightGenerator()
    js = gen.generate_summary(sample_data(), format="json")
    assert js["summary"]["task_agent"]["completed"] == 1
    assert js["summary"]["schedule_agent"]["total_hours"] == 2.5
    assert js["summary"]["health_agent"]["avg_sleep_score"] == 75.0
    assert "g1" in js["summary"]["goals"]
    assert "schedule_hours" in js["summary"]["trends"]


@pytest.mark.parametrize("fmt", ["bad", "xml"])
def test_generate_summary_invalid_format(fmt):
    gen = InsightGenerator()
    with pytest.raises(ValueError):
        gen.generate_summary({}, format=fmt)


def test_generate_report_with_notification(monkeypatch):
    gen = InsightGenerator()
    sent = {}

    class DummyClient:
        def send_notification(self, payload):
            sent.update(payload)

    client = DummyClient()
    report = gen.generate_report(
        user_id="u1",
        period="daily",
        agent_data=sample_data(),
        mcp_client=client,
        notify=True,
    )

    assert "# Insight Report" in report
    assert sent["user_id"] == "u1"
    assert sent["period"] == "daily"
    assert sent["type"] == "insight_report"


def test_compile_daily_weekly_aliases():
    gen = InsightGenerator()
    daily = gen.compile_daily(sample_data())
    weekly = gen.compile_weekly(sample_data())
    assert daily == weekly


def test_generate_daily_weekly_helpers_notify():
    gen = InsightGenerator()
    sent = {}

    class DummyClient:
        def send_notification(self, payload):
            sent.setdefault(payload["period"], []).append(payload)

    client = DummyClient()
    daily = gen.generate_daily_report(
        user_id="u", agent_data=sample_data(), mcp_client=client
    )
    weekly = gen.generate_weekly_report(
        user_id="u", agent_data=sample_data(), mcp_client=client
    )

    assert "# Insight Report" in daily
    assert "# Insight Report" in weekly
    assert sent["daily"][0]["type"] == "insight_report"
    assert sent["weekly"][0]["period"] == "weekly"


def test_compile_cache_reuse():
    gen = InsightGenerator()
    data = sample_data()

    first = gen.compile(data)
    second = gen.compile(data)
    assert first is second

    gen.clear_cache()
    third = gen.compile(data)
    assert third is not first


def test_disk_cache_json_persistence(tmp_path):
    cache_file = tmp_path / "cache.json"
    gen = InsightGenerator(cache_path=str(cache_file))
    summary = gen.compile(sample_data())
    assert cache_file.exists()

    gen2 = InsightGenerator(cache_path=str(cache_file))
    key = gen2._cache_key(sample_data())
    assert key in gen2._cache
    summary2 = gen2.compile(sample_data())
    assert summary2 == summary


def test_disk_cache_sqlite_persistence(tmp_path):
    cache_file = tmp_path / "cache.sqlite"
    gen = InsightGenerator(cache_path=str(cache_file))
    gen.compile(sample_data())

    gen2 = InsightGenerator(cache_path=str(cache_file))
    key = gen2._cache_key(sample_data())
    assert key in gen2._cache


def test_cache_pruning_in_memory():
    gen = InsightGenerator(max_entries=2)
    gen.compile({"a": []})
    gen.compile({"b": []})
    assert len(gen._cache) == 2
    gen.compile({"c": []})
    assert len(gen._cache) == 2
    key_a = gen._cache_key({"a": []})
    assert key_a not in gen._cache


def test_cache_pruning_persisted(tmp_path):
    cache_file = tmp_path / "cache.json"
    gen = InsightGenerator(cache_path=str(cache_file), max_entries=2)
    data_a = {"id": 1}
    data_b = {"id": 2}
    data_c = {"id": 3}
    gen.compile({"a": [data_a]})
    gen.compile({"b": [data_b]})
    gen.compile({"c": [data_c]})

    gen2 = InsightGenerator(cache_path=str(cache_file), max_entries=2)
    key_a = gen2._cache_key({"a": [data_a]})
    assert key_a not in gen2._cache
    assert len(gen2._cache) == 2
