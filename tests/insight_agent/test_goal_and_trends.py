import pytest
from insight_agent.insight_generator import InsightGenerator
from insight_agent.insight_agent import InsightAgent


def sample_goal_data():
    return {
        "schedule_agent": [
            {"id": 1, "start": "2025-06-01T09:00:00Z", "end": "2025-06-01T10:00:00Z"},
            {"id": 2, "start": "2025-06-02T09:00:00Z", "end": "2025-06-02T11:00:00Z"},
        ],
        "task_agent": [
            {"id": "t1", "status": "done", "completed_at": "2025-06-01T12:00:00Z"},
            {"id": "t2", "status": "todo"},
        ],
        "health_agent": [
            {"sleep_score": 80.0, "date": "2025-06-01"},
            {"sleep_score": 70.0, "date": "2025-06-02"},
        ],
        "goals": [
            {"id": "g1", "target": 10, "current": 5},
            {"id": "g2", "progress": 20},
        ],
    }


def test_goal_progress_and_trends_compiled():
    gen = InsightGenerator()
    summary = gen.compile(sample_goal_data())

    assert summary["goals"]["g1"] == 50.0
    assert summary["goals"]["g2"] == 20.0
    trends = summary["trends"]
    assert trends["schedule_hours"]["2025-06-01"] == 1.0
    assert trends["schedule_hours"]["2025-06-02"] == 2.0
    assert trends["completed_tasks"]["2025-06-01"] == 1
    assert trends["avg_sleep_score"]["2025-06-01"] == 80.0


def test_agent_generate_report_json_includes_trends():
    agent = InsightAgent(mcp_client=None)
    report = agent.generate_report(
        user_id="u1",
        period="daily",
        agent_data=sample_goal_data(),
        format="json",
    )
    assert report["summary"]["goals"]["g1"] == 50.0
    assert "schedule_hours" in report["summary"]["trends"]

