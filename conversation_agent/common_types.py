from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
import datetime

class UserInput(BaseModel):
    text: str
    timestamp: datetime.datetime = Field(default_factory=datetime.datetime.utcnow)
    metadata: Optional[Dict[str, Any]] = None

class AgentResponse(BaseModel):
    text: str
    timestamp: datetime.datetime = Field(default_factory=datetime.datetime.utcnow)
    metadata: Optional[Dict[str, Any]] = None

class ConversationTurn(BaseModel):
    user_input: Optional[UserInput] = None
    agent_response: Optional[AgentResponse] = None
    timestamp: datetime.datetime = Field(default_factory=datetime.datetime.utcnow)

class ConversationState(BaseModel):
    conversation_id: str
    history: List[ConversationTurn] = Field(default_factory=list)
    current_context: Dict[str, Any] = Field(default_factory=dict)
    last_interaction_time: datetime.datetime = Field(default_factory=datetime.datetime.utcnow)
