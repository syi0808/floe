from typing import Any, AsyncIterable, Dict
from google.genai import types
from abc import ABC, abstractmethod


class BaseAgent(ABC):
    SUPPORTED_CONTENT_TYPES: list[str]

    @abstractmethod
    def invoke(self, query: str, session_id: str) -> str:
        pass

    @abstractmethod
    async def stream(
        self, query: str, session_id: str
    ) -> AsyncIterable[Dict[str, Any]]:
        pass
