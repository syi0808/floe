import os
import time
from typing import Any, Dict, Optional

import requests


class MCPClient:
    """Simple HTTP client for interacting with the MCP server."""

    def __init__(
        self,
        base_url: str,
        token: str,
        token_type: str = "Bearer",
        timeout: int = 5,
        max_retries: int = 3,
        backoff_factor: float = 0.5,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.token_type = token_type
        self.timeout = timeout
        self.max_retries = max_retries
        self.backoff_factor = backoff_factor
        self.session = requests.Session()

    @classmethod
    def from_env(cls) -> "MCPClient":
        """Create a client using environment variables."""
        base_url = os.getenv("MCP_BASE_URL", "http://localhost:8000")
        token = os.getenv("MCP_ACCESS_TOKEN", "")
        token_type = os.getenv("MCP_TOKEN_TYPE", "Bearer")
        return cls(base_url=base_url, token=token, token_type=token_type)

    # Internal helpers -------------------------------------------------
    def _headers(self) -> Dict[str, str]:
        return {
            "Authorization": f"{self.token_type} {self.token}",
            "Content-Type": "application/json",
        }

    def _request(self, method: str, path: str, **kwargs) -> Any:
        url = f"{self.base_url}{path}"
        headers = kwargs.pop("headers", self._headers())
        for attempt in range(self.max_retries + 1):
            try:
                resp = self.session.request(
                    method, url, headers=headers, timeout=self.timeout, **kwargs
                )
                resp.raise_for_status()
                return resp.json() if resp.content else None
            except requests.RequestException:
                if attempt == self.max_retries:
                    raise
                time.sleep(self.backoff_factor * (2**attempt))
        return None

    # Public API methods ----------------------------------------------
    def invoke_service(self, service_name: str, payload: Dict[str, Any]) -> Any:
        data = {"service_name": service_name, "payload": payload}
        return self._request("POST", "/mcp/invoke_service", json=data)

    def add_memory(self, user_id: str, memory_item: Dict[str, Any]) -> Any:
        return self._request("POST", f"/mcp/memories/{user_id}", json=memory_item)

    def search_memories(self, user_id: str, query: str, top_k: int = 5) -> Any:
        params = {"query": query, "top_k": str(top_k)}
        return self._request(
            "GET", f"/mcp/memories/{user_id}/search", params=params
        )

    def send_reply(
        self,
        user_id: str,
        session_id: str,
        channel_type: str,
        content: str,
        target_details: Optional[Dict[str, Any]] = None,
    ) -> Any:
        data = {
            "user_id": user_id,
            "session_id": session_id,
            "channel_type": channel_type,
            "content": content,
        }
        if target_details:
            data["target_details"] = target_details
        return self._request("POST", "/mcp/send_reply", json=data)

    def send_notification(self, notification: Dict[str, Any]) -> Any:
        return self._request("POST", "/mcp/notifications", json=notification)
