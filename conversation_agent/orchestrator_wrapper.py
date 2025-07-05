from typing import Dict, Any, List, Optional
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse
from .conversation_agent import ConversationAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent
from mcp import MCPClient

class ConversationAgentWrapper(BaseAgent):
    """Adapter to use ConversationAgent with the OrchestrationEngine."""

    def __init__(
        self,
        memory_manager: Optional[MemoryManagerAgent] = None,
        mcp_client: MCPClient | None = None,
        user_id: str | None = None,
    ) -> None:
        self.agent = ConversationAgent(memory_manager=memory_manager, mcp_client=mcp_client)

        # Preload history if a memory manager and user_id are provided
        if memory_manager is not None and user_id is not None:
            self.agent.load_history_from_memory(user_id)

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

    # MCP helper methods -------------------------------------------------
    def invoke_service(self, service_name: str, payload: Dict[str, Any]):
        return self.agent.invoke_service(service_name, payload)

    def add_memory(self, user_id: str, memory_item: Dict[str, Any]):
        return self.agent.add_memory(user_id, memory_item)

    def search_memories(self, user_id: str, query: str, top_k: int = 5):
        return self.agent.search_memories(user_id, query, top_k)

    def send_reply(
        self,
        user_id: str,
        session_id: str,
        channel_type: str,
        content: str,
        target_details: Dict[str, Any] | None = None,
    ):
        return self.agent.send_reply(
            user_id,
            session_id,
            channel_type,
            content,
            target_details,
        )

    def send_notification(self, notification: Dict[str, Any]):
        return self.agent.send_notification(notification)
