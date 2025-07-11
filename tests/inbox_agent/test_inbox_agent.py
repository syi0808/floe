from unittest.mock import patch, MagicMock

from inbox_agent.email_connectors import GmailConnector, OutlookConnector, TokenStore
from datetime import datetime, timedelta
import json
from inbox_agent.email_processor import summarize_email
from inbox_agent.inbox_agent import InboxAgent


@patch("inbox_agent.email_connectors.requests.get")
def test_gmail_list_emails(mock_get):
    mock_get.return_value.json.return_value = {"messages": [{"id": "1"}]}
    mock_get.return_value.raise_for_status.return_value = None

    connector = GmailConnector("token")
    result = connector.list_emails("inbox", limit=1)

    mock_get.assert_called_once()
    assert result == [{"id": "1"}]


@patch("inbox_agent.email_connectors.requests.get")
def test_outlook_get_email_body(mock_get):
    mock_get.return_value.json.return_value = {"body": {"content": "Hello"}}
    mock_get.return_value.raise_for_status.return_value = None

    connector = OutlookConnector("token")
    body = connector.get_email_body("abc")

    mock_get.assert_called_once()
    assert body == "Hello"


@patch("inbox_agent.email_processor.openai.OpenAI")
def test_summarize_email(mock_openai):
    mock_client = MagicMock()
    mock_openai.return_value = mock_client
    choice = MagicMock()
    choice.message.content = "summary"
    mock_client.chat.completions.create.return_value.choices = [choice]

    summary = summarize_email("long text", max_length=50)
    assert summary == "summary"


def test_inbox_agent_fetch_email():
    gmail = MagicMock()
    gmail.list_emails.return_value = [{"id": "1"}]
    agent = InboxAgent(gmail_connector=gmail)

    resp = agent.process({"query": "test", "limit": 1}, user_id="u1")

    gmail.list_emails.assert_called_with(query="test", limit=1)
    assert resp["status"] == "success"
    assert resp["data"]["threads"] == [{"id": "1"}]


@patch("inbox_agent.inbox_agent.summarize_email", return_value="sum")
def test_inbox_agent_summarize_thread(mock_sum):
    gmail = MagicMock()
    gmail.get_email_body.return_value = "body"
    agent = InboxAgent(gmail_connector=gmail)

    resp = agent.process({"threadId": "t1"}, user_id="u1")

    gmail.get_email_body.assert_called_with("t1")
    assert resp["data"]["summary"] == "sum"


@patch("inbox_agent.email_connectors.requests.post")
def test_gmail_refresh_token(mock_post):
    mock_post.return_value.json.return_value = {"access_token": "new"}
    mock_post.return_value.raise_for_status.return_value = None
    conn = GmailConnector(
        access_token=None, refresh_token="r", client_id="c", client_secret="s"
    )
    with patch("inbox_agent.email_connectors.requests.get") as mock_get:
        mock_get.return_value.json.return_value = {"messages": []}
        mock_get.return_value.raise_for_status.return_value = None
        conn.list_emails()
    mock_post.assert_called_once()


@patch("inbox_agent.email_connectors.requests.get")
def test_gmail_expired_token_triggers_refresh(mock_get):
    mock_get.return_value.json.return_value = {"messages": []}
    mock_get.return_value.raise_for_status.return_value = None

    conn = GmailConnector("old", refresh_token="r", client_id="c", client_secret="s")
    conn.token_expires_at = datetime.utcnow() - timedelta(seconds=10)
    with patch.object(conn, "_refresh_token") as mock_refresh:
        conn.list_emails()
        mock_refresh.assert_called_once()


@patch("inbox_agent.email_connectors.requests.post")
def test_outlook_refresh_token(mock_post):
    mock_post.return_value.json.return_value = {"access_token": "new"}
    mock_post.return_value.raise_for_status.return_value = None
    conn = OutlookConnector(
        access_token=None, refresh_token="r", client_id="c", client_secret="s"
    )
    with patch("inbox_agent.email_connectors.requests.get") as mock_get:
        mock_get.return_value.json.return_value = {"value": []}
        mock_get.return_value.raise_for_status.return_value = None
        conn.list_emails()
    mock_post.assert_called_once()


@patch("inbox_agent.email_connectors.requests.get")
def test_outlook_expired_token_triggers_refresh(mock_get):
    mock_get.return_value.json.return_value = {"value": []}
    mock_get.return_value.raise_for_status.return_value = None

    conn = OutlookConnector("old", refresh_token="r", client_id="c", client_secret="s")
    conn.token_expires_at = datetime.utcnow() - timedelta(seconds=10)
    with patch.object(conn, "_refresh_token") as mock_refresh:
        conn.list_emails()
        mock_refresh.assert_called_once()


@patch("inbox_agent.email_connectors.requests.get")
def test_gmail_refresh_on_401(mock_get):
    resp_401 = MagicMock()
    resp_401.status_code = 401
    resp_401.json.return_value = {}
    resp_ok = MagicMock()
    resp_ok.status_code = 200
    resp_ok.json.return_value = {"messages": []}
    mock_get.side_effect = [resp_401, resp_ok]

    conn = GmailConnector("old", refresh_token="r", client_id="c", client_secret="s")
    with patch.object(conn, "_refresh_token") as mock_refresh:
        conn.list_emails()
        mock_refresh.assert_called_once()


@patch("inbox_agent.email_connectors.requests.get")
def test_outlook_refresh_on_401(mock_get):
    resp_401 = MagicMock()
    resp_401.status_code = 401
    resp_401.json.return_value = {}
    resp_ok = MagicMock()
    resp_ok.status_code = 200
    resp_ok.json.return_value = {"value": []}
    mock_get.side_effect = [resp_401, resp_ok]

    conn = OutlookConnector("old", refresh_token="r", client_id="c", client_secret="s")
    with patch.object(conn, "_refresh_token") as mock_refresh:
        conn.list_emails()
        mock_refresh.assert_called_once()


def test_token_store_load_save(tmp_path):
    token_file = tmp_path / "tokens.json"
    token_file.write_text('{"access_token": "a", "refresh_token": "r"}')
    store = TokenStore(token_file)
    connector = GmailConnector(token_store=store, client_id="c", client_secret="s")
    assert connector.access_token == "a"
    connector.refresh_token = "r"
    connector.token_expires_at = datetime.utcnow() - timedelta(seconds=10)
    with patch("inbox_agent.email_connectors.requests.post") as mock_post:
        mock_post.return_value.json.return_value = {"access_token": "new"}
        mock_post.return_value.raise_for_status.return_value = None
        mock_post.return_value.status_code = 200
        with patch("inbox_agent.email_connectors.requests.get") as mock_get:
            mock_get.return_value.json.return_value = {"messages": []}
            mock_get.return_value.raise_for_status.return_value = None
            connector.list_emails()
    saved = json.loads(token_file.read_text())
    assert saved["access_token"] == "new"


def test_inbox_agent_automatic_connectors(tmp_path):
    gmail_file = tmp_path / "g.json"
    outlook_file = tmp_path / "o.json"
    gmail_file.write_text('{"access_token": "g"}')
    outlook_file.write_text('{"access_token": "o"}')
    agent = InboxAgent(
        gmail_token_store=TokenStore(gmail_file),
        outlook_token_store=TokenStore(outlook_file),
    )
    assert isinstance(agent.gmail, GmailConnector)
    assert isinstance(agent.outlook, OutlookConnector)
