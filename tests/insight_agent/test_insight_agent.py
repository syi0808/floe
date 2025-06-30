from insight_agent.insight_agent import InsightAgent


def sample_data():
    return {
        "schedule_agent": [
            {"id": 1, "duration_hours": 1.0},
        ],
    }


def test_generate_report_helper():
    sent = {}

    class DummyClient:
        def send_notification(self, payload):
            sent.update(payload)

    agent = InsightAgent(mcp_client=DummyClient())
    report = agent.generate_report(
        user_id="u1",
        period="daily",
        agent_data=sample_data(),
        notify=True,
    )

    assert "# Insight Report" in report
    assert sent["user_id"] == "u1"
    assert sent["period"] == "daily"


def test_process_invokes_generate_report():
    class DummyClient:
        def __init__(self):
            self.payload = None

        def send_notification(self, payload):
            self.payload = payload

    client = DummyClient()
    agent = InsightAgent(mcp_client=client)
    entities = {"period": "weekly", "data": sample_data(), "notify": True}
    resp = agent.process(entities, user_id="user")

    assert resp["status"] == "success"
    assert client.payload["period"] == "weekly"

