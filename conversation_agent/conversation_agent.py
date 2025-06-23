import datetime
from typing import Optional

from conversation_agent.input_handler import InputHandler
from conversation_agent.dialogue_manager import DialogueManager
from conversation_agent.intent_recognizer import IntentRecognizer
from conversation_agent.response_generator import ResponseGenerator
from conversation_agent.common_types import AgentResponse, UserInput, ConversationTurn
from memory_manager_agent.memory_manager import MemoryManagerAgent

class ConversationAgent:
    """Simple conversational agent orchestrating input, intent and responses.

    Optionally integrates with ``MemoryManagerAgent`` to persist conversation
    history across sessions.
    """

    def __init__(self, memory_manager: Optional[MemoryManagerAgent] = None):
        self.input_handler = InputHandler()
        self.dialogue_manager = DialogueManager()
        self.intent_recognizer = IntentRecognizer()
        self.response_generator = ResponseGenerator()
        self.memory_manager = memory_manager

    def _load_history(self, user_id: str, top_k: int = 20) -> None:
        """Populate ``DialogueManager`` history from ``MemoryManagerAgent``."""
        if not self.memory_manager:
            return

        memories = self.memory_manager.get_context_for_agent(
            user_id=user_id,
            agent_name="conversation_agent",
            query_text="conversation_history",
            top_k=top_k,
        )

        for item in memories:
            if item.get("type") != "conversation_turn":
                continue
            content = item.get("content", {})
            user_text = content.get("user")
            agent_text = content.get("agent")
            ts_str = item.get("timestamp")
            try:
                ts = datetime.datetime.fromisoformat(ts_str) if ts_str else datetime.datetime.utcnow()
            except ValueError:
                ts = datetime.datetime.utcnow()

            user_input = UserInput(text=user_text or "", timestamp=ts)
            agent_response = AgentResponse(text=agent_text or "", timestamp=ts)
            turn = ConversationTurn(
                user_input=user_input,
                agent_response=agent_response,
                timestamp=ts,
            )
            self.dialogue_manager.state.history.append(turn)

    def _store_turn(self, user_id: str, user_input: UserInput, agent_response: AgentResponse) -> None:
        """Persist a conversation turn to memory."""
        if not self.memory_manager:
            return
        memory_item = {
            "type": "conversation_turn",
            "content": {
                "user": user_input.text,
                "agent": agent_response.text,
            },
            "timestamp": agent_response.timestamp.isoformat(),
        }
        self.memory_manager.add_memory(user_id, memory_item)

    def handle_message(self, text: str, user_id: Optional[str] = None) -> AgentResponse:
        if user_id and self.memory_manager and not self.dialogue_manager.get_conversation_history():
            # Populate history if this is a new instance
            self._load_history(user_id)

        user_input = self.input_handler.process_input(text)
        self.dialogue_manager.add_user_input(user_input)

        intent = self.intent_recognizer.recognize_intent(user_input.text)
        language = None
        if user_input.metadata:
            language = user_input.metadata.get("language")

        response_text = self.response_generator.generate_response(
            intent, language=language or "en"
        )

        agent_response = AgentResponse(text=response_text)
        self.dialogue_manager.add_agent_response(agent_response)

        if user_id and self.memory_manager:
            self._store_turn(user_id, user_input, agent_response)

        return agent_response
