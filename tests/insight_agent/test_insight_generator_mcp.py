import json
import pytest
from insight_agent.insight_generator import InsightGenerator
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

def test_notification_payload(monkeypatch):
    monkeypatch.setenv("MCP_BASE_URL", "http://mcp")
    monkeypatch.setenv("MCP_ACCESS_TOKEN", "tkn")
    client = MCPClient.from_env()

    sent = {}

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        sent['url'] = url
        sent['json'] = kwargs.get('json')
        return DummyResponse({})

    monkeypatch.setattr(client.session, "request", fake_request)

    gen = InsightGenerator()
    gen.generate_daily_report("user", {}, mcp_client=client)

    assert sent['url'] == "http://mcp/mcp/notifications"
    assert sent['json']['user_id'] == 'user'
    assert sent['json']['type'] == 'insight_report'


def test_weekly_helper_payload(monkeypatch):
    monkeypatch.setenv("MCP_BASE_URL", "http://mcp")
    monkeypatch.setenv("MCP_ACCESS_TOKEN", "tkn")
    client = MCPClient.from_env()

    sent = {}

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        sent['url'] = url
        sent['json'] = kwargs.get('json')
        return DummyResponse({})

    monkeypatch.setattr(client.session, "request", fake_request)

    gen = InsightGenerator()
    gen.generate_weekly_report("user", {}, mcp_client=client)

    assert sent['url'] == "http://mcp/mcp/notifications"
    assert sent['json']['user_id'] == 'user'
    assert sent['json']['period'] == 'weekly'

