import unittest
from conversation_agent.response_generator import ResponseGenerator

class TestResponseGenerator(unittest.TestCase):
    def setUp(self):
        self.generator = ResponseGenerator()

    def test_known_intent(self):
        self.assertEqual(
            self.generator.generate_response("greeting"),
            "Hello! How can I assist you today?",
        )

    def test_unknown_intent(self):
        self.assertEqual(
            self.generator.generate_response("nonexistent"),
            "I'm not sure how to respond to that.",
        )

if __name__ == "__main__":
    unittest.main()
