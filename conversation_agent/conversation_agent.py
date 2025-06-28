import datetime
from typing import Optional, Dict, Any

from conversation_agent.input_handler import InputHandler
from conversation_agent.dialogue_manager import DialogueManager
from conversation_agent.intent_recognizer import IntentRecognizer
from conversation_agent.response_generator import ResponseGenerator
from conversation_agent.common_types import AgentResponse, UserInput, ConversationTurn
from memory_manager_agent.memory_manager import MemoryManagerAgent
from mcp import MCPClient

class ConversationAgent:
    """Simple conversational agent orchestrating input, intent and responses.

    Optionally integrates with ``MemoryManagerAgent`` to persist conversation
    history across sessions.
    """

    def __init__(
        self,
        memory_manager: Optional[MemoryManagerAgent] = None,
        mcp_client: MCPClient | None = None,
    ):
        self.input_handler = InputHandler()
        self.dialogue_manager = DialogueManager()
        self.intent_recognizer = IntentRecognizer()
        self.response_generator = ResponseGenerator()
        self.mcp_client = mcp_client or MCPClient.from_env()
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
            is_clarification = item.get("is_clarification", False)
            ts_str = item.get("timestamp")
            try:
                ts = datetime.datetime.fromisoformat(ts_str) if ts_str else datetime.datetime.utcnow()
            except ValueError:
                ts = datetime.datetime.utcnow()

            self.dialogue_manager.state.turn_counter += 1
            user_input = UserInput(text=user_text or "", timestamp=ts)
            agent_response = AgentResponse(text=agent_text or "", timestamp=ts)
            turn = ConversationTurn(
                turn_id=self.dialogue_manager.state.turn_counter,
                user_input=user_input,
                agent_response=agent_response,
                timestamp=ts,
                is_clarification=is_clarification,
            )
            self.dialogue_manager.state.history.append(turn)
            self.dialogue_manager.state.last_interaction_time = ts

    def _store_turn(self, user_id: str, user_input: UserInput, agent_response: AgentResponse) -> None:
        """Persist a conversation turn to memory."""
        if not self.memory_manager:
            return
        last_turn = self.dialogue_manager.state.history[-1]
        memory_item = {
            "type": "conversation_turn",
            "content": {
                "user": user_input.text,
                "agent": agent_response.text,
            },
            "timestamp": agent_response.timestamp.isoformat(),
            "turn_id": last_turn.turn_id,
            "is_clarification": last_turn.is_clarification,
        }
        self.memory_manager.add_memory(user_id, memory_item)

    def handle_message(self, text: str, user_id: Optional[str] = None) -> AgentResponse:
        if user_id and self.memory_manager and not self.dialogue_manager.get_conversation_history():
            # Populate history if this is a new instance
            self._load_history(user_id)

        user_input = self.input_handler.process_input(text)
        self.dialogue_manager.add_user_input(user_input)

        # Resolve any pending clarification with this input
        if self.dialogue_manager.state.waiting_for_clarification:
            self.dialogue_manager.resolve_clarification(user_input.text)
            # We don't return here; continue to process normally after resolving

        intent = self.intent_recognizer.recognize_intent(user_input.text)
        language = None
        if user_input.metadata:
            language = user_input.metadata.get("language")

        confidence = 1.0 if intent != "unknown" else 0.5

        if self.dialogue_manager.check_clarification_needed(
            confidence, False, "Could you clarify your request?"
        ):
            agent_response = AgentResponse(text=self.dialogue_manager.state.pending_question)
            self.dialogue_manager.add_agent_response(agent_response, is_clarification=True)
            if user_id and self.memory_manager:
                self._store_turn(user_id, user_input, agent_response)
            return agent_response

        response_text = self.response_generator.generate_response(
            intent, language=language or "en"
        )

        agent_response = AgentResponse(text=response_text)
        self.dialogue_manager.add_agent_response(agent_response)

        if user_id and self.memory_manager:
            self._store_turn(user_id, user_input, agent_response)

        return agent_response

    # MCP helper methods -------------------------------------------------
    def invoke_service(self, service_name: str, payload: Dict[str, Any]):
        return self.mcp_client.invoke_service(service_name, payload)

    def add_memory(self, user_id: str, memory_item: Dict[str, Any]):
        return self.mcp_client.add_memory(user_id, memory_item)

    def search_memories(self, user_id: str, query: str, top_k: int = 5):
        return self.mcp_client.search_memories(user_id, query, top_k)

    def send_reply(
        self,
        user_id: str,
        session_id: str,
        channel_type: str,
        content: str,
        target_details: Dict[str, Any] | None = None,
    ):
        return self.mcp_client.send_reply(
            user_id,
            session_id,
            channel_type,
            content,
            target_details,
        )

    def send_notification(self, notification: Dict[str, Any]):
        return self.mcp_client.send_notification(notification)
