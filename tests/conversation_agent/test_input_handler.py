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
        self.assertIn("language", user_input.metadata)
        self.assertIsInstance(user_input.metadata.get("language"), str)

    def test_process_input_with_whitespace(self):
        text = "  leading and trailing spaces  "
        user_input = self.handler.process_input(text)
        self.assertEqual(user_input.text, "leading and trailing spaces")
        self.assertIn("language", user_input.metadata)

    def test_process_input_with_metadata(self):
        text = "Query with metadata"
        metadata = {"source": "test_case", "user_id": 123}
        user_input = self.handler.process_input(text, metadata=metadata)
        self.assertEqual(user_input.text, "Query with metadata")
        self.assertEqual(user_input.metadata.get("source"), metadata["source"])
        self.assertEqual(user_input.metadata.get("user_id"), metadata["user_id"])
        self.assertIn("language", user_input.metadata)

    def test_language_detection_korean(self):
        text = "안녕하세요"
        user_input = self.handler.process_input(text)
        self.assertEqual(user_input.metadata.get("language"), "ko")

if __name__ == '__main__':
    unittest.main()
