from typing import Dict, Any, List, Optional

class MockMemoryManagerAgent:
    """
    Mock implementation of the MemoryManagerAgent for integration testing.
    """

    def __init__(self):
        self._memory_storage: Dict[str, List[Dict[str, Any]]] = {}  # user_id -> list of memory items
        print("MockMemoryManagerAgent initialized.")

    def get_context_for_agent(self, user_id: str, agent_name: str, query_text: str, top_k: int = 3) -> List[Dict[str, Any]]:
        """
        Retrieves relevant context from mock memory.
        """
        print(f"MockMemoryManagerAgent: get_context_for_agent called for user '{user_id}', agent '{agent_name}', query '{query_text[:50]}...'")
        if user_id in self._memory_storage and self._memory_storage[user_id]:
            # Return up to top_k most recent items for this user
            user_memories = self._memory_storage[user_id]
            return user_memories[-top_k:]
        return []

    def add_memory(self, user_id: str, memory_item: Dict[str, Any]):
        """
        Adds a new memory item to the mock storage.
        """
        required_keys = {'type', 'content'} # Basic validation, can be expanded
        if not required_keys.issubset(memory_item.keys()):
            # In a real scenario, might raise ValueError or handle more gracefully
            print(f"Warning: memory_item missing required keys. Got: {memory_item.keys()}")
            return

        if user_id not in self._memory_storage:
            self._memory_storage[user_id] = []
        self._memory_storage[user_id].append(memory_item)
        print(f"MockMemoryManagerAgent: Added memory for user '{user_id}': {memory_item}")

    def get_user_memories(self, user_id: str) -> List[Dict[str, Any]]:
        """Helper method for tests to retrieve all memories for a user."""
        return self._memory_storage.get(user_id, [])

    def clear_memory(self, user_id: Optional[str] = None):
        """
        Helper method for tests to clear memory.
        If user_id is provided, clears memory for that user.
        Otherwise, clears all memory.
        """
        if user_id:
            if user_id in self._memory_storage:
                del self._memory_storage[user_id]
        else:
            self._memory_storage.clear()
