from abc import ABC, abstractmethod
from typing import Dict, Any, List
from __future__ import annotations
from typing import Dict, Any, List, TYPE_CHECKING

if TYPE_CHECKING:                        # only during type-checking
    from .orchestrator_core import AgentResponse

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
