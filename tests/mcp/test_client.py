import json
import pytest
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


def test_invoke_service_success(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tkn")

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        assert url == "http://mcp/mcp/invoke_service"
        return DummyResponse({"ok": True})

    monkeypatch.setattr(client.session, "request", fake_request)
    resp = client.invoke_service("svc", {"a": 1})
    assert resp == {"ok": True}


def test_request_retries(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tkn", max_retries=1)
    calls = []

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        calls.append(1)
        if len(calls) == 1:
            raise requests.exceptions.ConnectionError("fail")
        return DummyResponse({"ok": True})

    monkeypatch.setattr(client.session, "request", fake_request)
    resp = client.invoke_service("svc", {})
    assert resp == {"ok": True}
    assert len(calls) == 2
