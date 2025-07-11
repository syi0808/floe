import json
import pytest
import requests

from mcp.client import MCPClient


class DummyResponse:
    def __init__(self, data=None, status=200):
        self._data = data
        self.status_code = status
        self.content = json.dumps(data).encode() if data is not None else b""

    def json(self):
        return self._data

    def raise_for_status(self):
        if self.status_code >= 400:
            raise requests.HTTPError(response=self)


def test_send_reply_builds_payload(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tok")

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        assert method == "POST"
        assert url == "http://mcp/mcp/send_reply"
        assert kwargs["json"] == {
            "user_id": "u1",
            "session_id": "s1",
            "channel_type": "chat",
            "content": "hello",
            "target_details": {"thread": "t1"},
        }
        return DummyResponse({"ok": True})

    monkeypatch.setattr(client.session, "request", fake_request)
    resp = client.send_reply(
        "u1", "s1", "chat", "hello", target_details={"thread": "t1"}
    )
    assert resp == {"ok": True}


def test_send_notification(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tok")

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        assert method == "POST"
        assert url == "http://mcp/mcp/notifications"
        assert kwargs["json"] == {"type": "alert", "message": "hi"}
        return DummyResponse({"sent": True})

    monkeypatch.setattr(client.session, "request", fake_request)
    resp = client.send_notification({"type": "alert", "message": "hi"})
    assert resp == {"sent": True}


def test_send_reply_http_error(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tok")

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        return DummyResponse({}, status=500)

    monkeypatch.setattr(client.session, "request", fake_request)
    with pytest.raises(requests.HTTPError):
        client.send_reply("u1", "s1", "chat", "oops")


def test_send_notification_http_error(monkeypatch):
    client = MCPClient(base_url="http://mcp", token="tok")

    def fake_request(method, url, headers=None, timeout=5, **kwargs):
        return DummyResponse({}, status=400)

    monkeypatch.setattr(client.session, "request", fake_request)
    with pytest.raises(requests.HTTPError):
        client.send_notification({"type": "alert"})
