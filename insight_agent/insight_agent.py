from typing import Dict, Any, List
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse

class InsightAgent(BaseAgent):
    """Stub agent for generating insights and reports."""

    @property
    def name(self) -> str:
        return "insight_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["generate_insight_report"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        return AgentResponse(
            status="success",
            data={"received": entities, "user": user_id},
            message="Insight report generated.",
            source_agent=self.name,
        )
