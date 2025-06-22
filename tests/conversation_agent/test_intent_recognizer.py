import unittest
from conversation_agent.intent_recognizer import IntentRecognizer

class TestIntentRecognizer(unittest.TestCase):
    def setUp(self):
        self.recognizer = IntentRecognizer()

    def test_greeting(self):
        self.assertEqual(self.recognizer.recognize_intent("Hello there"), "greeting")

    def test_goodbye(self):
        self.assertEqual(self.recognizer.recognize_intent("bye"), "goodbye")

    def test_request_task_creation(self):
        self.assertEqual(
            self.recognizer.recognize_intent("Please add task buy milk"),
            "request_task_creation",
        )

    def test_question(self):
        self.assertEqual(
            self.recognizer.recognize_intent("What time is it?"),
            "ask_question",
        )

    def test_unknown(self):
        self.assertEqual(self.recognizer.recognize_intent("Just stating"), "unknown")

if __name__ == "__main__":
    unittest.main()
