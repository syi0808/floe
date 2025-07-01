import unittest
from conversation_agent.conversation_agent import ConversationAgent, UserInput, AgentResponse
from tests.integration.mocks.mock_memory_manager_agent import MockMemoryManagerAgent

class TestHistoryMethods(unittest.TestCase):
    def test_store_and_load_explicit_methods(self):
        mm = MockMemoryManagerAgent()
        user_id = "explicit_user"
        agent = ConversationAgent(memory_manager=mm)

        # Manually create a conversation turn
        agent.dialogue_manager.add_user_input(UserInput(text="Hi"))
        agent.dialogue_manager.add_agent_response(AgentResponse(text="Hello"))
        agent.store_last_turn_to_memory(user_id)

        # New agent loads the stored turn
        agent2 = ConversationAgent(memory_manager=mm)
        agent2.load_history_from_memory(user_id)
        history = agent2.dialogue_manager.get_conversation_history()

        self.assertEqual(len(history), 1)
        self.assertEqual(history[0].user_input.text, "Hi")
        self.assertEqual(history[0].agent_response.text, "Hello")

if __name__ == "__main__":
    unittest.main()
