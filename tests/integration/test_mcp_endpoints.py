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


def test_mcp_client_memory_crud(monkeypatch):
    from mcp import MCPClient
    client = MCPClient.from_env()

    def fake_get(method, url, headers=None, timeout=5, **kwargs):
        assert method == "GET"
        assert url == "http://mcp.test/mcp/memories/u1/m1"
        return DummyResponse({"id": "m1"})

    monkeypatch.setattr(client.session, "request", fake_get)
    assert client.get_memory("u1", "m1") == {"id": "m1"}

    def fake_put(method, url, headers=None, timeout=5, **kwargs):
        assert method == "PUT"
        assert url == "http://mcp.test/mcp/memories/u1/m1"
        assert kwargs.get("json") == {"data": "x"}
        return DummyResponse({"ok": True})

    monkeypatch.setattr(client.session, "request", fake_put)
    assert client.update_memory("u1", "m1", {"data": "x"}) == {"ok": True}

    def fake_delete(method, url, headers=None, timeout=5, **kwargs):
        assert method == "DELETE"
        assert url == "http://mcp.test/mcp/memories/u1/m1"
        return DummyResponse({})

    monkeypatch.setattr(client.session, "request", fake_delete)
    assert client.delete_memory("u1", "m1") == {}
