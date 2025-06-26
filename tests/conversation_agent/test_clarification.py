import unittest
from conversation_agent.conversation_agent import ConversationAgent

class TestClarificationFlow(unittest.TestCase):
    def test_clarification_trigger_and_resolution(self):
        agent = ConversationAgent()
        # Send an unknown message to trigger clarification
        resp = agent.handle_message("unrecognized input")
        self.assertIn("clarify", resp.text.lower())
        self.assertTrue(agent.dialogue_manager.state.waiting_for_clarification)

        # Respond to clarification
        resp2 = agent.handle_message("Hello")
        self.assertFalse(agent.dialogue_manager.state.waiting_for_clarification)
        self.assertEqual(resp2.text, "Hello! How can I assist you today?")

if __name__ == "__main__":
    unittest.main()
