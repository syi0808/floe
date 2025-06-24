import pytest
from unittest.mock import patch, MagicMock # MagicMock can still be useful for simpler mocks
import os
import json

from orchestrator_agent.intent_analyzer import extract_intent_and_entities
# Tool classes might still be imported if their names/schemas are directly referenced in assertions,
# but not strictly necessary for mocking the LiteLLM call itself if we only check output strings.
from orchestrator_agent.intent_analyzer import ExtractScheduleInfoTool, CreateTaskTool

# Helper mock classes for LiteLLM response structure
class MockLiteLLMFunctionCall:
    def __init__(self, name, arguments_dict):
        self.name = name
        # LiteLLM typically returns arguments as a JSON string
        self.arguments = json.dumps(arguments_dict)

class MockLiteLLMToolCall:
    def __init__(self, function_name, function_args_dict, type="function"):
        self.type = type
        self.function = MockLiteLLMFunctionCall(function_name, function_args_dict)

class MockLiteLLMMessage:
    def __init__(self, content=None, tool_calls=None):
        self.content = content
        self.tool_calls = tool_calls if tool_calls is not None else []

class MockLiteLLMChoice:
    def __init__(self, message_content=None, tool_calls_list=None): # Pass a list of (name, args_dict) tuples for tool_calls
        tool_calls = []
        if tool_calls_list:
            for tc_name, tc_args_dict in tool_calls_list:
                tool_calls.append(MockLiteLLMToolCall(tc_name, tc_args_dict))
        self.message = MockLiteLLMMessage(content=message_content, tool_calls=tool_calls)

class MockLiteLLMResponse:
    def __init__(self, choices_data): # choices_data is a list of (message_content, tool_calls_list) tuples
        self.choices = []
        for msg_content, tc_list in choices_data:
            self.choices.append(MockLiteLLMChoice(message_content=msg_content, tool_calls_list=tc_list))

# Test for extract_schedule_info intent using LiteLLM
@patch('os.getenv')
@patch('litellm.completion')
def test_extract_intent_schedule_info_litellm(mock_litellm_completion, mock_os_getenv):
    mock_os_getenv.return_value = "test-model" # Mock LITELLM_MODEL_NAME

    mock_tool_call_data = [('extract_schedule_info', {'title': 'Meeting', 'date': 'Tomorrow'})]
    mock_litellm_completion.return_value = MockLiteLLMResponse(
       choices_data=[(None, mock_tool_call_data)] # No direct message content, one tool call
    )

    user_query = "Schedule a meeting for tomorrow"
    result = extract_intent_and_entities(user_query)

    mock_litellm_completion.assert_called_once()
    # We can inspect mock_litellm_completion.call_args here if needed for more detail

    # Verify os.getenv was called for LITELLM_MODEL_NAME
    mock_os_getenv.assert_any_call("LITELLM_MODEL_NAME")

    assert result == {'intent': 'extract_schedule_info', 'entities': {'title': 'Meeting', 'date': 'Tomorrow'}}

# Test for create_task intent using LiteLLM
@patch('os.getenv')
@patch('litellm.completion')
def test_extract_intent_create_task_litellm(mock_litellm_completion, mock_os_getenv):
    mock_os_getenv.return_value = "test-model"

    mock_tool_call_data = [('create_task', {'task_description': 'Buy milk'})]
    mock_litellm_completion.return_value = MockLiteLLMResponse(
        choices_data=[(None, mock_tool_call_data)]
    )

    user_query = "Remind me to buy milk"
    result = extract_intent_and_entities(user_query)

    mock_litellm_completion.assert_called_once()
    mock_os_getenv.assert_any_call("LITELLM_MODEL_NAME")
    assert result == {'intent': 'create_task', 'entities': {'task_description': 'Buy milk'}}

# Test for general_conversation intent using LiteLLM
@patch('os.getenv')
@patch('litellm.completion')
def test_extract_intent_general_conversation_litellm(mock_litellm_completion, mock_os_getenv):
    mock_os_getenv.return_value = "test-model"

    mock_litellm_completion.return_value = MockLiteLLMResponse(
        choices_data=[("Hello there!", None)] # Message content, no tool calls
    )

    user_query = "Hi"
    result = extract_intent_and_entities(user_query)

    mock_litellm_completion.assert_called_once()
    mock_os_getenv.assert_any_call("LITELLM_MODEL_NAME")
    assert result == {'intent': 'general_conversation', 'response_text': 'Hello there!'}

# Test for error handling when LiteLLM API call fails
@patch('os.getenv')
@patch('litellm.completion')
def test_extract_intent_error_litellm(mock_litellm_completion, mock_os_getenv):
    mock_os_getenv.return_value = "test-model"
    mock_litellm_completion.side_effect = Exception("LiteLLM API error")

    user_query = "Some query that causes an error"
    result = extract_intent_and_entities(user_query)

    mock_litellm_completion.assert_called_once()
    mock_os_getenv.assert_any_call("LITELLM_MODEL_NAME")
    assert result == {'error': 'LiteLLM API call failed: LiteLLM API error'}

# Test for missing LITELLM_MODEL_NAME environment variable
@patch('os.getenv')
@patch('litellm.completion') # Still need to patch completion as it might be called if getenv doesn't cause early exit
def test_missing_lite_llm_model_name(mock_litellm_completion, mock_os_getenv):
    # Simulate os.getenv returning None for LITELLM_MODEL_NAME
    mock_os_getenv.return_value = None

    user_query = "Any query"
    result = extract_intent_and_entities(user_query)

    # Ensure os.getenv was called for LITELLM_MODEL_NAME
    mock_os_getenv.assert_any_call("LITELLM_MODEL_NAME")

    # Ensure litellm.completion was NOT called because model name is missing
    mock_litellm_completion.assert_not_called()

    assert result == {'error': 'LITELLM_MODEL_NAME environment variable not set.'}

# Test for JSONDecodeError when parsing tool arguments
@patch('os.getenv')
@patch('litellm.completion')
def test_extract_intent_json_decode_error_litellm(mock_litellm_completion, mock_os_getenv):
    mock_os_getenv.return_value = "test-model"

    # Create a mock tool call with invalid JSON arguments
    class MockInvalidArgsFunctionCall:
        def __init__(self, name):
            self.name = name
            self.arguments = "this is not valid json" # Invalid JSON string

    class MockInvalidArgsToolCall:
        def __init__(self, function_name):
            self.type = "function"
            self.function = MockInvalidArgsFunctionCall(function_name)

    mock_tool_call = MockInvalidArgsToolCall(function_name='extract_schedule_info')

    # Create a response that includes this malformed tool call
    mock_response_message = MockLiteLLMMessage(tool_calls=[mock_tool_call])
    mock_response_choice = MockLiteLLMChoice(message_content=None) # Need to init choice correctly
    mock_response_choice.message = mock_response_message # Manually set the message with invalid tool call

    mock_litellm_completion.return_value = MockLiteLLMResponse(choices_data=[]) # Start empty
    mock_litellm_completion.return_value.choices = [mock_response_choice] # Then inject the problematic choice


    user_query = "Schedule a meeting"
    result = extract_intent_and_entities(user_query)

    mock_litellm_completion.assert_called_once()
    mock_os_getenv.assert_any_call("LITELLM_MODEL_NAME")

    assert 'error' in result
    assert "Failed to parse arguments for tool extract_schedule_info" in result['error']

# Test for case where LLM responds with a tool call but no valid content or arguments
@patch('os.getenv')
@patch('litellm.completion')
def test_extract_intent_empty_tool_call_litellm(mock_litellm_completion, mock_os_getenv):
    mock_os_getenv.return_value = "test-model"

    # Simulate a tool call response where arguments might be missing or message content is also null
    class MockEmptyFunctionCall:
        def __init__(self, name):
            self.name = name
            self.arguments = None # Or an empty string, depending on how LiteLLM might represent this

    class MockEmptyToolCall:
        def __init__(self, function_name):
            self.type = "function"
            self.function = MockEmptyFunctionCall(function_name)

    # This setup simulates a scenario where tool_calls list is present and contains an item,
    # but that item itself doesn't lead to successful parsing or content generation.
    # The intent_analyzer has a path for this:
    # "LLM responded with a tool call but no valid content or arguments."
    # This is tricky to simulate precisely without knowing exact LiteLLM internal error states,
    # but we can aim for the condition where tool_calls is present, but no arguments are parsable
    # AND message.content is also empty.

    # Scenario 1: Tool call present, but arguments are None
    mock_tool_call_no_args = MockEmptyToolCall(function_name='extract_schedule_info')
    mock_message_with_empty_tool_call = MockLiteLLMMessage(tool_calls=[mock_tool_call_no_args], content=None)

    temp_choice = MockLiteLLMChoice() # Create a choice
    temp_choice.message = mock_message_with_empty_tool_call # Set its message

    mock_litellm_completion.return_value = MockLiteLLMResponse(choices_data=[]) # Init with empty
    mock_litellm_completion.return_value.choices = [temp_choice] # Set the choices list

    user_query = "Schedule something"
    result = extract_intent_and_entities(user_query)

    mock_litellm_completion.assert_called_once()
    mock_os_getenv.assert_any_call("LITELLM_MODEL_NAME")

    assert 'error' in result
    assert "Failed to parse arguments for tool extract_schedule_info" in result['error']
