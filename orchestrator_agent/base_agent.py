from abc import ABC, abstractmethod
from typing import Dict, Any, List
from .orchestrator_core import AgentResponse # Assuming AgentResponse is in orchestrator_core

class BaseAgent(ABC):
    @abstractmethod
    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        pass

    @property
    @abstractmethod
    def name(self) -> str:
        pass

    @property
    @abstractmethod
    def supported_intents(self) -> List[str]:
        pass
