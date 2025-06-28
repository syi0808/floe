import unittest
import datetime
import uuid
from conversation_agent.dialogue_manager import DialogueManager
from conversation_agent.common_types import UserInput, AgentResponse, ConversationState, ConversationTurn

class TestDialogueManager(unittest.TestCase):

    def setUp(self):
        self.manager = DialogueManager(conversation_id="test-convo")

    def test_initial_state(self):
        state = self.manager.get_current_state()
        self.assertIsInstance(state, ConversationState)
        self.assertEqual(state.conversation_id, "test-convo")
        self.assertEqual(len(state.history), 0)
        self.assertEqual(state.current_context, {})
        self.assertIsInstance(state.last_interaction_time, datetime.datetime)

    def test_add_user_input(self):
        user_text = "Hello bot"
        input_time = datetime.datetime.utcnow()
        # Ensure the UserInput model is correctly imported and used
        user_input = UserInput(text=user_text, timestamp=input_time)

        self.manager.add_user_input(user_input)
        state = self.manager.get_current_state()

        self.assertEqual(len(state.history), 1)
        turn = state.history[0]
        self.assertIsInstance(turn, ConversationTurn)
        self.assertEqual(turn.user_input, user_input)
        self.assertIsNone(turn.agent_response)
        self.assertEqual(state.last_interaction_time, input_time)
        self.assertEqual(state.current_context.get('last_user_message'), user_text)

    def test_add_agent_response(self):
        user_text = "User says hi"
        user_input = UserInput(text=user_text)
        self.manager.add_user_input(user_input) # Need a turn to add response to

        agent_text = "Agent says hi back"
        response_time = datetime.datetime.utcnow()
        # Ensure the AgentResponse model is correctly imported and used
        agent_response = AgentResponse(text=agent_text, timestamp=response_time)

        self.manager.add_agent_response(agent_response)
        state = self.manager.get_current_state()

        self.assertEqual(len(state.history), 1)
        turn = state.history[0]
        self.assertEqual(turn.agent_response, agent_response)
        # Assuming turn timestamp updates to agent response time if agent response is last
        self.assertEqual(turn.timestamp, response_time)
        self.assertEqual(state.last_interaction_time, response_time)
        self.assertEqual(state.current_context.get('last_agent_message'), agent_text)

    def test_add_multiple_turns(self):
        user_input1 = UserInput(text="First message")
        self.manager.add_user_input(user_input1)
        agent_response1 = AgentResponse(text="First reply")
        self.manager.add_agent_response(agent_response1)

        user_input2 = UserInput(text="Second message")
        self.manager.add_user_input(user_input2)
        agent_response2 = AgentResponse(text="Second reply")
        self.manager.add_agent_response(agent_response2)

        state = self.manager.get_current_state()
        self.assertEqual(len(state.history), 2)
        self.assertEqual(state.history[0].user_input, user_input1)
        self.assertEqual(state.history[0].agent_response, agent_response1)
        self.assertEqual(state.history[1].user_input, user_input2)
        self.assertEqual(state.history[1].agent_response, agent_response2)

    def test_reset_conversation(self):
        user_input = UserInput(text="Some message")
        self.manager.add_user_input(user_input)

        original_id = self.manager.get_current_state().conversation_id
        self.manager.reset_conversation(conversation_id="new-test-convo")

        state = self.manager.get_current_state()
        self.assertEqual(state.conversation_id, "new-test-convo")
        self.assertNotEqual(state.conversation_id, original_id)
        self.assertEqual(len(state.history), 0)
        self.assertEqual(state.current_context, {})

    def test_reset_conversation_generates_uuid(self):
        self.manager.reset_conversation()
        state = self.manager.get_current_state()
        self.assertIsNotNone(state.conversation_id)
        try:
            uuid.UUID(state.conversation_id)
        except ValueError:
            self.fail("Generated conversation ID is not a valid UUID")

    def test_clarification_turn_recording(self):
        user_input = UserInput(text="Hello")
        self.manager.add_user_input(user_input)
        needed = self.manager.check_clarification_needed(0.1, False, "Clarify?")
        self.assertTrue(needed)
        response = AgentResponse(text=self.manager.state.pending_question)
        self.manager.add_agent_response(response, is_clarification=True)
        turn = self.manager.get_conversation_history()[-1]
        self.assertTrue(turn.is_clarification)
        self.assertEqual(self.manager.state.pending_clarification_turn_id, turn.turn_id)
        self.manager.resolve_clarification("done")
        self.assertIsNone(self.manager.state.pending_clarification_turn_id)

if __name__ == '__main__':
    unittest.main()
