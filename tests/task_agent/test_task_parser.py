import unittest
from unittest.mock import patch, MagicMock
import os

# Ensure the task_agent module can be found
import sys
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../..')))

from task_agent.task_parser import parse_task_request, CreateTaskFromDetailsTool

# Import AGENTS_SDK_AVAILABLE to check if the actual SDK is expected to be used.
# This helps in deciding how to mock and what to expect.
try:
    from task_agent.task_parser import AGENTS_SDK_AVAILABLE
except ImportError:
    # If AGENTS_SDK_AVAILABLE itself is not found, assume SDK is not available for testing.
    AGENTS_SDK_AVAILABLE = False


class TestTaskParser(unittest.TestCase):

    @patch('task_agent.task_parser.Runner')
    @patch('task_agent.task_parser.Agent')
    @patch.object(CreateTaskFromDetailsTool, '__call__') # More specific patch for the tool's output
    def test_parse_simple_task_query_successful_extraction(self, mock_tool_call, mock_agent_constructor, mock_runner_run_sync):
        """
        Tests parsing a simple task query where the mocked agent framework
        successfully extracts details via the tool.
        """
        if not AGENTS_SDK_AVAILABLE:
            self.skipTest("Skipping test: agents SDK is not available or not correctly mocked.")

        # --- Arrange ---
        natural_language_query = "Remind me to buy milk tomorrow"

        # Mock the response from the CreateTaskFromDetailsTool.__call__ method
        # This is what the `tool_input` in `result.tool_calls[0].tool_input` would contain
        expected_task_details = {
            "description": "buy milk",
            "due_date": "tomorrow",
            "priority": None,
            "project": None,
            "source": "nlp"
        }
        # mock_tool_call.return_value = expected_task_details # This is not needed as the tool_input is directly assigned below

        # Mock the result object from Runner.run_sync
        # It should contain a structure that parse_task_request expects,
        # typically a list of tool_calls, where each call has a tool_name and tool_input.
        mock_tool_call_obj = MagicMock()
        mock_tool_call_obj.tool_name = "create_task_from_details"
        mock_tool_call_obj.tool_input = expected_task_details # This is the crucial part

        mock_run_sync_result = MagicMock()
        mock_run_sync_result.tool_calls = [mock_tool_call_obj]
        mock_runner_run_sync.return_value = mock_run_sync_result

        # Mock the Agent constructor to avoid issues if it has complex dependencies
        mock_agent_instance = MagicMock()
        mock_agent_constructor.return_value = mock_agent_instance

        # --- Act ---
        actual_details = parse_task_request(natural_language_query)

        # --- Assert ---
        # Verify that Agent was constructed with the tool
        mock_agent_constructor.assert_called_once()
        args, kwargs = mock_agent_constructor.call_args
        self.assertTrue(any(isinstance(tool, CreateTaskFromDetailsTool) for tool in kwargs.get('tools', [])))

        # Verify that Runner.run_sync was called with the agent and query
        mock_runner_run_sync.assert_called_once_with(agent=mock_agent_instance, user_input=natural_language_query)

        # Verify the returned details
        self.assertEqual(actual_details, expected_task_details)
        # We don't need to assert mock_tool_call was called because we are checking the final output of parse_task_request
        # which relies on the tool_input being correctly propagated.

    @patch('task_agent.task_parser.Runner')
    def test_parse_task_query_tool_not_called(self, mock_runner_run_sync):
        """
        Tests the scenario where the agent framework does not call the tool
        (e.g., the query is not recognized as a task).
        """
        if not AGENTS_SDK_AVAILABLE:
            self.skipTest("Skipping test: agents SDK is not available or not correctly mocked.")

        natural_language_query = "What is the weather today?"

        # Mock Runner.run_sync to return a result where no tool_calls were made
        mock_run_sync_result = MagicMock()
        mock_run_sync_result.tool_calls = [] # No tool calls
        mock_run_sync_result.final_output = "The weather is sunny." # Example final output
        mock_runner_run_sync.return_value = mock_run_sync_result

        actual_details = parse_task_request(natural_language_query)

        self.assertIsNone(actual_details, "Expected None when tool is not called")

    def test_parse_task_request_sdk_unavailable(self):
        """
        Tests the behavior when the AGENTS_SDK_AVAILABLE is False.
        parse_task_request should return None.
        """
        if AGENTS_SDK_AVAILABLE:
            self.skipTest("Skipping test: AGENTS_SDK_AVAILABLE is True, this test is for when it's False.")

        # Temporarily patch AGENTS_SDK_AVAILABLE to False for this specific test case,
        # if it was True globally but we want to test the False path.
        # If it's already False due to import errors, this patch is still safe.
        with patch('task_agent.task_parser.AGENTS_SDK_AVAILABLE', False):
            # Also need to ensure CreateTaskFromDetailsTool is patched if AGENTS_SDK_AVAILABLE is False in the module
            # or that the module re-evaluates AGENTS_SDK_AVAILABLE when parse_task_request is called.
            # The current implementation of parse_task_request checks AGENTS_SDK_AVAILABLE at its start.
            actual_details = parse_task_request("any query")
            self.assertIsNone(actual_details)

if __name__ == '__main__':
    unittest.main()
