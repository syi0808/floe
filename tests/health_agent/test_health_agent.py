from health_agent.health_agent import HealthAgent


def test_process_log_steps():
    agent = HealthAgent()
    resp = agent.process({"steps": 500}, user_id="u")
    assert resp["status"] == "success"
    assert resp["data"]["received"]["steps"] == 500


def test_weekly_summary():
    agent = HealthAgent()
    agent.process({"steps": 300}, user_id="u")
    resp = agent.process({"weekly_summary": True}, user_id="u")
    assert "Avg sleep" in resp["data"]["summary"]
