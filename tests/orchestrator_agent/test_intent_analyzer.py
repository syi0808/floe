import pytest
from unittest.mock import patch, MagicMock
import os

from orchestrator_agent.intent_analyzer import extract_intent_and_entities, ExtractScheduleInfoTool, CreateTaskTool

# Test for extract_schedule_info intent
def test_extract_intent_schedule_info():
    mock_run_sync_result = MagicMock()
    mock_tool_call = MagicMock()
    mock_tool_call.tool_name = 'extract_schedule_info'
    mock_tool_call.tool_input = {'title': 'Meeting', 'date': 'Tomorrow'}
    mock_run_sync_result.tool_calls = [mock_tool_call]
    mock_run_sync_result.final_output = None

    with patch('agents.Runner.run_sync', return_value=mock_run_sync_result) as mock_run_sync:
        user_query = "Schedule a meeting for tomorrow"
        api_key = "fake_api_key"
        result = extract_intent_and_entities(user_query, api_key)

        mock_run_sync.assert_called_once()
        # We can add more specific assertions about the agent and tools if needed
        # For instance, checking the instructions passed to the Agent

        assert result == {'intent': 'extract_schedule_info', 'entities': {'title': 'Meeting', 'date': 'Tomorrow'}}
        assert os.environ["OPENAI_API_KEY"] == api_key

# Test for create_task intent
def test_extract_intent_create_task():
    mock_run_sync_result = MagicMock()
    mock_tool_call = MagicMock()
    mock_tool_call.tool_name = 'create_task'
    mock_tool_call.tool_input = {'task_description': 'Buy milk'}
    mock_run_sync_result.tool_calls = [mock_tool_call]
    mock_run_sync_result.final_output = None

    with patch('agents.Runner.run_sync', return_value=mock_run_sync_result) as mock_run_sync:
        user_query = "Remind me to buy milk"
        api_key = "fake_api_key_task" # Using a different key to ensure it's being set
        result = extract_intent_and_entities(user_query, api_key)

        mock_run_sync.assert_called_once()
        assert result == {'intent': 'create_task', 'entities': {'task_description': 'Buy milk'}}
        assert os.environ["OPENAI_API_KEY"] == api_key

# Test for general_conversation intent
def test_extract_intent_general_conversation():
    mock_run_sync_result = MagicMock()
    mock_run_sync_result.tool_calls = None # Or []
    mock_run_sync_result.final_output = "Hello there!"

    with patch('agents.Runner.run_sync', return_value=mock_run_sync_result) as mock_run_sync:
        user_query = "Hi"
        api_key = "fake_api_key_general"
        result = extract_intent_and_entities(user_query, api_key)

        mock_run_sync.assert_called_once()
        assert result == {'intent': 'general_conversation', 'response_text': 'Hello there!'}
        assert os.environ["OPENAI_API_KEY"] == api_key

# Test for error handling
def test_extract_intent_error():
    with patch('agents.Runner.run_sync', side_effect=Exception("API error")) as mock_run_sync:
        user_query = "Some query that causes an error"
        api_key = "fake_api_key_error"
        result = extract_intent_and_entities(user_query, api_key)

        mock_run_sync.assert_called_once()
        assert result == {'error': 'Could not determine intent: API error'}
        assert os.environ["OPENAI_API_KEY"] == api_key
