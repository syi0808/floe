import unittest

from conversation_agent.conversation_agent import ConversationAgent
from tests.integration.mocks.mock_memory_manager_agent import MockMemoryManagerAgent

class TestConversationMemoryPersistence(unittest.TestCase):
    def test_history_persists_via_memory_manager(self):
        mm = MockMemoryManagerAgent()
        user_id = "user_memory"
        agent = ConversationAgent(memory_manager=mm)
        agent.handle_message("Hello", user_id=user_id)
        agent.handle_message("Bye", user_id=user_id)

        # New agent instance loads previous history
        agent2 = ConversationAgent(memory_manager=mm)
        agent2.handle_message("Hi again", user_id=user_id)
        history = agent2.dialogue_manager.get_conversation_history()
        self.assertEqual(len(history), 3)
        self.assertEqual(history[0].user_input.text, "Hello")
        self.assertEqual(history[1].user_input.text, "Bye")
        self.assertEqual(history[2].user_input.text, "Hi again")

if __name__ == "__main__":
    unittest.main()
