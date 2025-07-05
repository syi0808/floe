import unittest
from conversation_agent.conversation_agent import ConversationAgent, UserInput, AgentResponse
from conversation_agent.orchestrator_wrapper import ConversationAgentWrapper
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

    def test_wrapper_loads_history_on_init(self):
        mm = MockMemoryManagerAgent()
        user_id = "wrapper_user"

        # Pre-populate memory with one turn
        ts = "2025-07-02T00:00:00"
        mm.add_memory(user_id, {
            "type": "conversation_turn",
            "content": {"user": "Hi", "agent": "Hello"},
            "timestamp": ts,
        })

        wrapper = ConversationAgentWrapper(memory_manager=mm, user_id=user_id)
        history = wrapper.agent.dialogue_manager.get_conversation_history()

        self.assertEqual(len(history), 1)
        self.assertEqual(history[0].user_input.text, "Hi")
        self.assertEqual(history[0].agent_response.text, "Hello")

if __name__ == "__main__":
    unittest.main()
