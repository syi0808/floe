from typing import Dict, Any, List, Optional
from orchestrator_agent.base_agent import BaseAgent  # For type hinting if MemoryManagerAgent itself becomes an agent
from mcp import MCPClient
# (import removed – not used in this module)

# Forward declaration or import of BaseMemoryModel if it were defined elsewhere
# For now, we'll use Dict[str, Any] for memory items.
# from .memory_store import BaseMemoryModel # Example if BaseMemoryModel exists

class MemoryManagerAgent: # Not inheriting from BaseAgent for now, as its primary role is a service
    """
    Manages long-term and short-term memory for the Floe AI assistant.
    This is a basic implementation focusing on the interface needed by OrchestratorAgent.
    """

    def __init__(self, mcp_client: MCPClient | None = None):
        # In a real implementation, this would initialize connections to
        # vector databases, etc.
        self.mcp_client = mcp_client or MCPClient.from_env()
        self._memory_storage: Dict[str, List[Dict[str, Any]]] = {}
        print("Basic MemoryManagerAgent initialized.")

    def get_context_for_agent(self, user_id: str, agent_name: str, query_text: str, top_k: int = 3) -> List[Dict[str, Any]]:
        """
        Retrieves relevant context from memory for a given agent and query.
        This is a mock implementation.

        Args:
            user_id: The identifier of the user.
            agent_name: The name of the agent requesting context.
            query_text: The user's query text or a summary of the current task.
            top_k: The number of top relevant memory items to retrieve.

        Returns:
            A list of memory items (dictionaries) deemed relevant.
            Returns an empty list if no relevant context is found or for mock purposes.
        """
        print(f"MemoryManagerAgent: get_context_for_agent called for user '{user_id}', agent '{agent_name}', query '{query_text[:50]}...'")

        # Mock functionality: Return some generic context or context based on user_id
        if user_id in self._memory_storage and self._memory_storage[user_id]:
            # Return up to top_k most recent items for this user as mock context
            user_memories = self._memory_storage[user_id]
            return user_memories[-top_k:]

        # Generic mock response if no specific memories or for demonstration
        mock_context = [
            {"type": "conversation_summary", "content": f"Previously discussed topic related to '{query_text[:20]}' for {agent_name}."},
            {"type": "user_preference", "content": "User prefers concise answers."}
        ]
        return mock_context[:top_k]

    def add_memory(self, user_id: str, memory_item: Dict[str, Any]):
        """
        Adds a new memory item for the given user.
        This is a mock implementation.

        Args:
            user_id: The identifier of the user.
            memory_item: A dictionary representing the memory to add.
                         Expected to have at least 'type' and 'content'.
        """
        required = {'type', 'content'}
        if not required.issubset(memory_item):
            raise ValueError(
                f"memory_item must contain keys {required}, got {set(memory_item)}"
            )

        if user_id not in self._memory_storage:
            self._memory_storage[user_id] = []
        self._memory_storage[user_id].append(memory_item)
        print(f"MemoryManagerAgent: Added memory for user '{user_id}': {memory_item}")

    # MCP helper methods -------------------------------------------------
    def invoke_service(self, service_name: str, payload: Dict[str, Any]):
        return self.mcp_client.invoke_service(service_name, payload)

    def post_memory_to_mcp(self, user_id: str, memory_item: Dict[str, Any]):
        return self.mcp_client.add_memory(user_id, memory_item)

    def search_memories_via_mcp(self, user_id: str, query: str, top_k: int = 5):
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
      
    def get_user_memories(self, user_id: str) -> List[Dict[str, Any]]:
        """Return all memories stored for ``user_id``."""
        return self._memory_storage.get(user_id, [])

    def clear_memory(self, user_id: Optional[str] = None):
        """Clear memory for a specific user or all users if ``user_id`` is ``None``."""
        if user_id:
            self._memory_storage.pop(user_id, None)
        else:
            self._memory_storage.clear()

# Example of how MemoryManagerAgent could itself be a BaseAgent (optional for now)
# class MemoryManagerAgentAsFloeAgent(BaseAgent, MemoryManagerAgent): # Multiple inheritance
#     def __init__(self):
#         BaseAgent.__init__(self) # Call BaseAgent constructor if it has one
#         MemoryManagerAgent.__init__(self) # Call MemoryManagerAgent constructor
#
#     @property
#     def name(self) -> str:
#         return "memory_manager_agent"
#
#     @property
#     def supported_intents(self) -> List[str]:
#         return ["store_memory", "retrieve_memory"] # Example intents
#
#     def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
#         intent = entities.get("memory_intent") # Assuming intent is passed within entities
#         if intent == "store_memory":
#             item_to_store = entities.get("item")
#             if item_to_store:
#                 self.add_memory(user_id, item_to_store)
#                 return AgentResponse(status="success", data={"stored": True}, message="Memory stored.", source_agent=self.name)
#             return AgentResponse(status="error", data={"stored": False}, message="No item provided to store.", source_agent=self.name)
#         elif intent == "retrieve_memory":
#             query = entities.get("query", "")
#             context = self.get_context_for_agent(user_id, "user_direct_query", query)
#             return AgentResponse(status="success", data={"retrieved_context": context}, message="Memory retrieved.", source_agent=self.name)
#         return AgentResponse(status="error", data=None, message="Unknown memory intent.", source_agent=self.name)
