import os
import json
import pytest
import requests

from schedule_agent.schedule_agent import ScheduleAgent
from conversation_agent.conversation_agent import ConversationAgent

class DummyResponse:
    def __init__(self, data, status=200):
        self._data = data
        self.status_code = status
        self.content = json.dumps(data).encode()

    def json(self):
        return self._data

    def raise_for_status(self):
        if self.status_code >= 400:
            raise requests.HTTPError(response=self)

@pytest.fixture(autouse=True)
def setup_env(monkeypatch):
    monkeypatch.setenv("MCP_BASE_URL", "http://mcp.test")
    monkeypatch.setenv("MCP_ACCESS_TOKEN", "abc123")
    monkeypatch.setenv("MCP_TOKEN_TYPE", "Bearer")


def test_schedule_agent_invoke_service(monkeypatch):
    agent = ScheduleAgent()

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        assert url == "http://mcp.test/mcp/invoke_service"
        assert headers.get("Authorization") == "Bearer abc123"
        return DummyResponse({"ok": True})

    monkeypatch.setattr(agent.mcp_client.session, "request", fake_request)
    resp = agent.invoke_service("demo", {"a": 1})
    assert resp == {"ok": True}


def test_conversation_agent_add_memory(monkeypatch):
    agent = ConversationAgent()

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        assert url == "http://mcp.test/mcp/memories/user42"
        assert headers.get("Authorization") == "Bearer abc123"
        return DummyResponse({})

    monkeypatch.setattr(agent.mcp_client.session, "request", fake_request)
    agent.add_memory("user42", {"type": "conversation_turn", "content": "hi"})
