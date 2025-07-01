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
    assert summary["task_agent"]["count"] == 2
    assert summary["task_agent"]["completed"] == 1
    assert summary["health_agent"]["count"] == 2
    assert summary["health_agent"]["avg_sleep_score"] == 75.0
    assert summary["goals"]["count"] == 2
    assert summary["goals"]["completed"] == 1
    assert "tasks_completed" in summary["trends"]


def test_generate_summary_markdown_multi():
    gen = InsightGenerator()
    md = gen.generate_summary(sample_data(), format="markdown")
    assert "# Insight Report" in md
    assert "**schedule_agent**" in md
    assert "2 entries" in md
    assert "completed" in md
    assert "avg sleep score" in md
    assert "## Goals" in md
    assert "g1" in md
    assert "## Trends" in md


def test_generate_summary_json_multi():
    gen = InsightGenerator()
    js = gen.generate_summary(sample_data(), format="json")
    assert js["summary"]["task_agent"]["completed"] == 1
    assert js["summary"]["schedule_agent"]["total_hours"] == 2.5
    assert js["summary"]["health_agent"]["avg_sleep_score"] == 75.0
    assert js["summary"]["goals"]["completed"] == 1
    assert "tasks_completed" in js["summary"]["trends"]


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
