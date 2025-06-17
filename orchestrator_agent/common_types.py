from typing import TypedDict, Optional, Any

class AgentResponse(TypedDict):
    status: str  # e.g., 'success', 'error'
    data: Optional[Any]
    message: Optional[str]
    source_agent: str
