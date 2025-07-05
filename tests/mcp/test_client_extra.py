import json
from unittest.mock import MagicMock
import requests

from mcp.client import MCPClient


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


def test_add_memory_success(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tok")

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        assert method == "POST"
        assert url == "http://mcp/mcp/memories/u1"
        assert kwargs["json"] == {"type": "note", "content": "hello"}
        return DummyResponse({"ok": True})

    monkeypatch.setattr(client.session, "request", fake_request)
    resp = client.add_memory("u1", {"type": "note", "content": "hello"})
    assert resp == {"ok": True}


def test_search_memories(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tok")

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        assert method == "GET"
        assert url == "http://mcp/mcp/memories/u1/search"
        assert kwargs["params"] == {"query": "hi", "top_k": "3"}
        return DummyResponse({"hits": []})

    monkeypatch.setattr(client.session, "request", fake_request)
    resp = client.search_memories("u1", "hi", top_k=3)
    assert resp == {"hits": []}


def test_multi_agent_shared_client_calls(monkeypatch):
    from conversation_agent.orchestrator_wrapper import ConversationAgentWrapper
    from schedule_agent.schedule_agent import ScheduleAgent
    from orchestrator_agent.orchestrator_core import OrchestrationEngine
    from memory_manager_agent.memory_manager import MemoryManagerAgent

    mock_client = MagicMock()
    memory_manager = MemoryManagerAgent(mcp_client=mock_client)
    engine = OrchestrationEngine(memory_manager_client=memory_manager)

    sa = ScheduleAgent(mcp_client=mock_client)
    conv = ConversationAgentWrapper(mcp_client=mock_client)

    engine.register_agent(sa)
    engine.register_agent(conv)

    sa.invoke_service("svc", {"p": 1})
    conv.add_memory("u1", {"type": "note", "content": "hi"})

    mock_client.invoke_service.assert_called_once_with("svc", {"p": 1})
    mock_client.add_memory.assert_called_once_with(
        "u1", {"type": "note", "content": "hi"}
    )
