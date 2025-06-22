import pytest
from schedule_agent.schedule_agent import ScheduleAgent

@pytest.fixture
def agent():
    return ScheduleAgent()


def test_process_success_all_fields(agent):
    entities = {
        "title": "Team Sync",
        "participants": ["Alice", "Bob"],
        "time": "2 PM",
        "date": "2025-07-01",
        "description": "Weekly catch-up"
    }
    user_id = "user123"
    response = agent.process(entities, user_id)

    assert response["status"] == "success"
    assert response["data"]["details"] == entities
    assert response["data"]["user_id"] == user_id
    assert response["data"]["event_id"].startswith("evt_mock_")
    assert "Successfully processed schedule request" in response["message"]
    assert response["source_agent"] == agent.name


@pytest.mark.parametrize("missing_key", ["title", "date"])
def test_process_missing_required_field(agent, missing_key):
    entities = {
        "title": "Project Kickoff",
        "participants": ["Alice", "Bob"],
        "time": "9 AM",
        "date": "2025-08-10",
    }
    entities.pop(missing_key)
    response = agent.process(entities, "user123")

    assert response["status"] == "error"
    assert response["data"]["missing_fields"] is True
    assert response["data"]["received_entities"] == entities
    assert "Missing required entities" in response["message"]
    assert response["source_agent"] == agent.name
