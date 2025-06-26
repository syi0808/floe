from typing import Dict, Any, List
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse
from mcp import MCPClient
from .insight_generator import InsightGenerator

class InsightAgent(BaseAgent):
    """Stub agent for generating insights and reports."""

    def __init__(self, mcp_client: MCPClient | None = None) -> None:
        super().__init__()
        self.mcp_client = mcp_client or MCPClient.from_env()

    @property
    def name(self) -> str:
        return "insight_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["generate_insight_report"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        gen = InsightGenerator()
        agent_data = entities.get("data", {})
        output_format = entities.get("format", "markdown")
        try:
            report = gen.generate_summary(agent_data, format=output_format)
        except ValueError as exc:
            return AgentResponse(
                status="error",
                data={"received": entities, "user": user_id},
                message=str(exc),
                source_agent=self.name,
            )

        return AgentResponse(
            status="success",
            data={"received": entities, "user": user_id, "report": report},
            message="Insight report generated.",
            source_agent=self.name,
        )

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
