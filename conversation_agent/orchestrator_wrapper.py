from typing import Dict, Any, List
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse
from .conversation_agent import ConversationAgent

class ConversationAgentWrapper(BaseAgent):
    """Adapter to use ConversationAgent with the OrchestrationEngine."""

    def __init__(self) -> None:
        self.agent = ConversationAgent()

    @property
    def name(self) -> str:
        return "conversation_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["general_conversation"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        # Expect the text under 'text' or 'response_text'
        text = entities.get("text") or entities.get("response_text") or ""
        resp = self.agent.handle_message(text)
        return AgentResponse(
            status="success",
            data={"response": resp.text},
            message="Conversation handled.",
            source_agent=self.name,
        )
