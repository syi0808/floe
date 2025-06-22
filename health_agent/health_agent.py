from typing import Dict, Any, List
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse

class HealthAgent(BaseAgent):
    """Stub agent for health data processing."""

    @property
    def name(self) -> str:
        return "health_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["log_health_data"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        return AgentResponse(
            status="success",
            data={"received": entities, "user": user_id},
            message="Health data logged.",
            source_agent=self.name,
        )
