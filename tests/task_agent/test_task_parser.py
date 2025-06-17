# tests/task_agent/test_task_parser.py
import unittest
from unittest.mock import patch, MagicMock

# Attempt to import the target function
# This structure assumes your project root is in PYTHONPATH when running tests
try:
    from task_agent.task_parser import parse_task_request, CreateTaskFromDetailsTool
except ImportError:
    # Fallback for environments where the path might be different during execution,
    # though ideally the test runner (e.g., pytest) handles PYTHONPATH.
    # This is more of a safeguard for direct script execution if needed.
    import sys
    import os
    # Adjust path to include the parent directory of 'task_agent'
    # This is a common pattern but might need adjustment based on your exact test execution setup
    sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../..')))
    from task_agent.task_parser import parse_task_request, CreateTaskFromDetailsTool


class TestTaskParser(unittest.TestCase):

    @patch('task_agent.task_parser.Runner.run_sync')
    def test_parse_full_task_details(self, mock_run_sync):
        # Mock the result from agents.Runner.run_sync
        mock_tool_input = {
            "description": "Finish the report for Project Alpha",
            "due_date": "next Friday",
            "priority": "high",
            "project": "Project Alpha"
        }
        mock_tool_call = MagicMock()
        mock_tool_call.tool_name = "create_task_from_details"
        mock_tool_call.tool_input = mock_tool_input

        mock_agent_result = MagicMock()
        mock_agent_result.tool_calls = [mock_tool_call]
        mock_run_sync.return_value = mock_agent_result

        query = "Add 'Finish the report for Project Alpha' to my tasks, it's due next Friday and is high priority under Project Alpha."
        result = parse_task_request(query)

        self.assertIsNotNone(result)
        self.assertEqual(result, mock_tool_input)
        # Verify that Agent and Runner were called (optional, but good for sanity check)
        # This requires knowing how Agent is instantiated within parse_task_request
        # For this, we might need to patch 'task_agent.task_parser.Agent' as well if we want to inspect its calls.
        # For now, focusing on run_sync is simpler.
        mock_run_sync.assert_called_once()

    @patch('task_agent.task_parser.Runner.run_sync')
    def test_parse_task_with_only_description(self, mock_run_sync):
        mock_tool_input = {
            "description": "Buy milk",
            "due_date": None,
            "priority": None,
            "project": None
        }
        mock_tool_call = MagicMock()
        mock_tool_call.tool_name = "create_task_from_details"
        mock_tool_call.tool_input = mock_tool_input

        mock_agent_result = MagicMock()
        mock_agent_result.tool_calls = [mock_tool_call]
        mock_run_sync.return_value = mock_agent_result

        query = "Remind me to buy milk."
        result = parse_task_request(query)

        self.assertIsNotNone(result)
        self.assertEqual(result, mock_tool_input)
        mock_run_sync.assert_called_once()

    @patch('task_agent.task_parser.Runner.run_sync')
    def test_parse_task_with_description_and_due_date(self, mock_run_sync):
        # This is what the LLM provides to the tool
        llm_provided_input_to_tool = {
             "description": "Submit expenses",
             "due_date": "tomorrow"
        }
        # The tool's __call__ method will be invoked with these, and defaults for others.
        # So, the tool_input captured in result.tool_calls[0].tool_input will be:
        actual_tool_output = CreateTaskFromDetailsTool()(**llm_provided_input_to_tool)

        mock_tool_call = MagicMock()
        mock_tool_call.tool_name = "create_task_from_details"
        mock_tool_call.tool_input = actual_tool_output # This is what parse_task_request will see

        mock_agent_result = MagicMock()
        mock_agent_result.tool_calls = [mock_tool_call]
        mock_run_sync.return_value = mock_agent_result

        query = "Submit expenses by tomorrow."
        result = parse_task_request(query)

        self.assertIsNotNone(result)
        # We expect the result to match actual_tool_output, which includes None for missing fields
        self.assertEqual(result["description"], "Submit expenses")
        self.assertEqual(result["due_date"], "tomorrow")
        self.assertIsNone(result["priority"]) # Based on tool's default behavior
        self.assertIsNone(result["project"])  # Based on tool's default behavior
        mock_run_sync.assert_called_once()

    @patch('task_agent.task_parser.Runner.run_sync')
    def test_no_tool_call_from_runner(self, mock_run_sync):
        # Simulate the runner not making any tool calls (e.g., irrelevant query)
        mock_agent_result = MagicMock()
        mock_agent_result.tool_calls = [] # No tool calls
        mock_agent_result.final_output = "I'm not sure how to help with that." # Example final output
        mock_run_sync.return_value = mock_agent_result

        query = "What's the weather like?"
        result = parse_task_request(query)

        self.assertIsNone(result) # Expect None if no relevant tool call
        mock_run_sync.assert_called_once()

    @patch('task_agent.task_parser.Runner.run_sync')
    def test_incorrect_tool_name_from_runner(self, mock_run_sync):
        # Simulate the runner calling an unexpected tool
        mock_tool_call = MagicMock()
        mock_tool_call.tool_name = "some_other_tool"
        mock_tool_call.tool_input = {"data": "some_data"}

        mock_agent_result = MagicMock()
        mock_agent_result.tool_calls = [mock_tool_call]
        mock_run_sync.return_value = mock_agent_result

        query = "Add 'Finish the report for Project Alpha' to my tasks." # A valid task query
        result = parse_task_request(query)

        self.assertIsNone(result) # Expect None as the correct tool wasn't called
        mock_run_sync.assert_called_once()

    @patch('task_agent.task_parser.Runner.run_sync')
    def test_runner_raises_exception(self, mock_run_sync):
        # Simulate an exception occurring during run_sync
        mock_run_sync.side_effect = Exception("LLM API error")

        query = "This query will cause an error."
        result = parse_task_request(query)

        self.assertIsNone(result) # Expect None if an exception occurs
        mock_run_sync.assert_called_once()

if __name__ == '__main__':
    unittest.main()
