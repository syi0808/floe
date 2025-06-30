from unittest.mock import patch, MagicMock
from inbox_agent.email_processor import (
    parse_email_actions,
    extract_email_actions,
    process_new_email,
)

@patch('inbox_agent.email_processor.openai.OpenAI')
def test_parse_email_actions_llm(mock_openai):
    mock_client = MagicMock()
    mock_openai.return_value = mock_client
    choice = MagicMock()
    choice.message.content = '[{"action": "CREATE_TASK", "details": {"text": "do"}}]'
    mock_client.chat.completions.create.return_value.choices = [choice]
    actions = parse_email_actions('1', 'subject', 'body', 'sender')
    assert actions == [{"action": "CREATE_TASK", "details": {"text": "do"}}]

@patch('inbox_agent.email_processor.extract_email_actions', return_value=[{"action": "PROPOSE_SCHEDULE", "details": {"text": "hi"}}])
@patch('inbox_agent.email_processor.summarize_email', return_value='sum')
def test_process_new_email_orchestrator(mock_sum, mock_extract):
    orchestrator = MagicMock()
    process_new_email('u', {"id": "1", "body_text": "b"}, orchestrator=orchestrator)
    orchestrator.route_request.assert_called_once()


def test_extract_email_actions_keyword_fallback():
    actions = extract_email_actions('1', 'Meeting', 'Please schedule a meeting', 'a@b.com')
    assert actions and actions[0]['action'] == 'PROPOSE_SCHEDULE'
