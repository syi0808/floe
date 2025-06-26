import datetime
import unittest
from conversation_agent.conversation_agent import ConversationAgent
from tests.integration.mocks.mock_memory_manager_agent import MockMemoryManagerAgent

class TestContextLoading(unittest.TestCase):
    def test_loads_previous_history_on_first_message(self):
        mm = MockMemoryManagerAgent()
        user_id = "history_user"
        ts = datetime.datetime.utcnow().isoformat()
        mm.add_memory(user_id, {
            "type": "conversation_turn",
            "content": {"user": "Hi", "agent": "Hello"},
            "timestamp": ts,
        })

        agent = ConversationAgent(memory_manager=mm)
        agent.handle_message("How are you?", user_id=user_id)
        history = agent.dialogue_manager.get_conversation_history()
        self.assertEqual(len(history), 2)
        self.assertEqual(history[0].user_input.text, "Hi")
        self.assertEqual(history[1].user_input.text, "How are you?")

if __name__ == "__main__":
    unittest.main()
