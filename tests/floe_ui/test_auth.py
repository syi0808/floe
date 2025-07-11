import pytest
from fastapi.testclient import TestClient

from floe_ui.server import app, AUTH_TOKEN, engine

client = TestClient(app)


def test_query_requires_token():
    resp = client.post("/query", json={"text": "hi", "user_id": "u"})
    assert resp.status_code == 401

    resp = client.post(
        "/query",
        json={"text": "hi", "user_id": "u"},
        headers={"Authorization": f"Bearer {AUTH_TOKEN}"},
    )
    assert resp.status_code == 200


def test_query_engine_error(monkeypatch):
    def fail(*args, **kwargs):
        raise RuntimeError("boom")

    monkeypatch.setattr(engine, "route_request", fail)
    resp = client.post(
        "/query",
        json={"text": "hi", "user_id": "u"},
        headers={"Authorization": f"Bearer {AUTH_TOKEN}"},
    )
    assert resp.status_code == 500
    data = resp.json()
    assert data["status"] == "error"
    assert "boom" in data["data"]["detail"]
