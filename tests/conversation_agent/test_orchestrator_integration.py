import unittest
from orchestrator_agent.orchestrator_core import OrchestrationEngine
from conversation_agent.orchestrator_wrapper import ConversationAgentWrapper
from memory_manager_agent.memory_manager import MemoryManagerAgent

class TestConversationOrchestratorIntegration(unittest.TestCase):
    def setUp(self):
        self.engine = OrchestrationEngine(memory_manager_client=MemoryManagerAgent())
        self.wrapper = ConversationAgentWrapper()
        self.engine.register_agent(self.wrapper)

    def test_greeting_through_orchestrator(self):
        intent = {"intent": "general_conversation", "response_text": "  Hello  "}
        resp = self.engine.route_request(intent, user_id="u1")
        data = resp["data"]["agent_response"]["data"]["response"]
        self.assertEqual(data, "Hello! How can I assist you today?")
        self.assertEqual(self.wrapper.agent.dialogue_manager.get_recent_user_messages()[-1], "Hello")

    def test_clarification_flow_via_orchestrator(self):
        intent = {"intent": "general_conversation", "response_text": "unrecognized input"}
        resp = self.engine.route_request(intent, user_id="u1")
        text = resp["data"]["agent_response"]["data"]["response"].lower()
        self.assertIn("clarify", text)
        self.assertTrue(self.wrapper.agent.dialogue_manager.state.waiting_for_clarification)

        intent2 = {"intent": "general_conversation", "response_text": "Hi"}
        resp2 = self.engine.route_request(intent2, user_id="u1")
        self.assertFalse(self.wrapper.agent.dialogue_manager.state.waiting_for_clarification)
        data2 = resp2["data"]["agent_response"]["data"]["response"]
        self.assertEqual(data2, "Hello! How can I assist you today?")
        self.assertEqual(len(self.wrapper.agent.dialogue_manager.get_conversation_history()), 2)

    def test_context_window_tracks_messages(self):
        msgs = ["Hi", "How are you?", "Great", "Thanks", "Bye", "Another"]
        for m in msgs:
            self.engine.route_request({"intent": "general_conversation", "response_text": m}, user_id="u1")
        ctx = self.wrapper.agent.dialogue_manager.get_context_window()
        self.assertEqual(ctx["user"][-1], "Another")
        self.assertEqual(len(ctx["user"]), self.wrapper.agent.dialogue_manager.context_window)

if __name__ == "__main__":
    unittest.main()
