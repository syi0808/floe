import datetime
import uuid
from typing import Optional, List, Dict, Any

from .common_types import UserInput, AgentResponse, ConversationTurn, ConversationState

class DialogueManager:
    def __init__(self, conversation_id: Optional[str] = None):
        self.state = ConversationState(
            conversation_id=conversation_id if conversation_id else str(uuid.uuid4())
        )

    def add_user_input(self, user_input: UserInput):
        """Adds user input to the conversation history."""
        turn = ConversationTurn(user_input=user_input, timestamp=user_input.timestamp)
        self.state.history.append(turn)
        self.state.last_interaction_time = user_input.timestamp
        # Basic state update: could be more sophisticated
        self.state.current_context['last_user_message'] = user_input.text

    def add_agent_response(self, agent_response: AgentResponse):
        """Adds agent response to the most recent turn or a new turn if necessary."""
        if not self.state.history or self.state.history[-1].agent_response is not None:
            # Create a new turn if history is empty or last turn already has a response
            turn = ConversationTurn(agent_response=agent_response, timestamp=agent_response.timestamp)
            self.state.history.append(turn)
        else:
            # Add to the last turn
            self.state.history[-1].agent_response = agent_response
            # Update timestamp of the turn if it makes sense, or keep separate user/agent timestamps
            # For simplicity, we assume AgentResponse timestamp is the primary for this update
            self.state.history[-1].timestamp = agent_response.timestamp

        self.state.last_interaction_time = agent_response.timestamp
        self.state.current_context['last_agent_message'] = agent_response.text


    def get_conversation_history(self) -> List[ConversationTurn]:
        return self.state.history

    def get_current_state(self) -> ConversationState:
        return self.state

    def reset_conversation(self, conversation_id: Optional[str] = None):
        """Resets the conversation state."""
        self.state = ConversationState(
            conversation_id=conversation_id if conversation_id else str(uuid.uuid4())
        )
        print(f"Conversation reset. New ID: {self.state.conversation_id}")


if __name__ == '__main__':
    # Example Usage
    manager = DialogueManager(conversation_id="test-convo-123")
    print(f"Initial State ID: {manager.get_current_state().conversation_id}")

    # Simulate user input
    first_input = UserInput(text="Hello there!", metadata={"source": "test"})
    manager.add_user_input(first_input)
    print(f"History after 1st user input: {manager.get_conversation_history()}")

    # Simulate agent response
    first_response = AgentResponse(text="Hi! How can I help you today?")
    manager.add_agent_response(first_response)
    print(f"History after 1st agent response: {manager.get_conversation_history()}")

    # Simulate another user input
    second_input = UserInput(text="Tell me a joke.")
    manager.add_user_input(second_input)
    print(f"History after 2nd user input: {manager.get_conversation_history()}")

    # Simulate another agent response
    second_response = AgentResponse(text="Why did the scarecrow win an award? Because he was outstanding in his field!")
    manager.add_agent_response(second_response)
    print(f"History after 2nd agent response: {manager.get_conversation_history()}")

    print(f"Final Context: {manager.get_current_state().current_context}")
    print(f"Last Interaction: {manager.get_current_state().last_interaction_time}")

    manager.reset_conversation()
    print(f"History after reset: {manager.get_conversation_history()}")
