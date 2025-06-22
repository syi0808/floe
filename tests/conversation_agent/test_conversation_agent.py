import unittest
from conversation_agent.conversation_agent import ConversationAgent

class TestConversationAgent(unittest.TestCase):
    def setUp(self):
        self.agent = ConversationAgent()

    def test_simple_greeting_flow(self):
        resp = self.agent.handle_message("Hello")
        self.assertEqual(resp.text, "Hello! How can I assist you today?")
        history = self.agent.dialogue_manager.get_conversation_history()
        self.assertEqual(len(history), 1)
        self.assertEqual(history[0].user_input.text, "Hello")
        self.assertEqual(history[0].agent_response.text, resp.text)

    def test_korean_greeting_flow(self):
        resp = self.agent.handle_message("안녕하세요")
        self.assertEqual(resp.text, "안녕하세요! 무엇을 도와드릴까요?")
        history = self.agent.dialogue_manager.get_conversation_history()
        self.assertEqual(history[-1].user_input.text, "안녕하세요")
        self.assertEqual(history[-1].agent_response.text, resp.text)

    def test_task_request_flow(self):
        resp = self.agent.handle_message("add task buy milk")
        self.assertIn("create a task", resp.text.lower())

if __name__ == "__main__":
    unittest.main()
