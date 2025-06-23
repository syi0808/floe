from typing import Dict, Any, List, Optional
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse
from .conversation_agent import ConversationAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent

class ConversationAgentWrapper(BaseAgent):
    """Adapter to use ConversationAgent with the OrchestrationEngine."""

    def __init__(self, memory_manager: Optional[MemoryManagerAgent] = None) -> None:
        self.agent = ConversationAgent(memory_manager=memory_manager)

    @property
    def name(self) -> str:
        return "conversation_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["general_conversation"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        try:
            # Expect the text under 'text' or 'response_text'
            text = entities.get("text") or entities.get("response_text") or ""
            resp = self.agent.handle_message(text, user_id=user_id)

            # Validate response has expected attributes
            if not hasattr(resp, 'text'):
                raise AttributeError("ConversationAgent response missing 'text' attribute")

            return AgentResponse(
                status="success",
                data={"response": resp.text},
                message="Conversation handled.",
                source_agent=self.name,
            )
        except Exception as e:
            return AgentResponse(
                status="error",
                data=None,
                message=f"Conversation processing failed: {str(e)}",
                source_agent=self.name,
            )
