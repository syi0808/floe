import unittest
import datetime
from conversation_agent.input_handler import InputHandler
from conversation_agent.common_types import UserInput

class TestInputHandler(unittest.TestCase):

    def setUp(self):
        self.handler = InputHandler()

    def test_process_input_simple(self):
        text = "Hello"
        user_input = self.handler.process_input(text)
        self.assertIsInstance(user_input, UserInput)
        self.assertEqual(user_input.text, "Hello")
        self.assertIsInstance(user_input.timestamp, datetime.datetime)
        self.assertIsNone(user_input.metadata)

    def test_process_input_with_whitespace(self):
        text = "  leading and trailing spaces  "
        user_input = self.handler.process_input(text)
        self.assertEqual(user_input.text, "leading and trailing spaces")

    def test_process_input_with_metadata(self):
        text = "Query with metadata"
        metadata = {"source": "test_case", "user_id": 123}
        user_input = self.handler.process_input(text, metadata=metadata)
        self.assertEqual(user_input.text, "Query with metadata")
        self.assertEqual(user_input.metadata, metadata)

if __name__ == '__main__':
    unittest.main()
