from typing import Dict, Any, List
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse

class InboxAgent(BaseAgent):
    """Simple agent to demonstrate inbox processing."""

    @property
    def name(self) -> str:
        return "inbox_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["process_email"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        return AgentResponse(
            status="success",
            data={"received": entities, "user": user_id},
            message="Inbox processed.",
            source_agent=self.name,
        )
