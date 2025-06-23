from __future__ import annotations

from typing import Any, Dict, List, Optional

from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse
from mcp import MCPClient

from .email_connectors import GmailConnector, OutlookConnector
from .email_processor import summarize_email


class InboxAgent(BaseAgent):
    """Agent responsible for fetching and processing email threads."""

    def __init__(
        self,
        gmail_connector: Optional[GmailConnector] = None,
        outlook_connector: Optional[OutlookConnector] = None,
    ) -> None:
        self.gmail = gmail_connector
        self.outlook = outlook_connector

    def __init__(self, mcp_client: MCPClient | None = None) -> None:
        super().__init__()
        self.mcp_client = mcp_client or MCPClient.from_env()

    @property
    def name(self) -> str:
        return "inbox_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["process_email", "fetch_email", "summarize_thread", "watch_thread"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        # fetch_email intent
        if "query" in entities or "limit" in entities:
            query = entities.get("query", "")
            limit = entities.get("limit", 10)
            threads: List[Dict[str, Any]] = []
            if self.gmail:
                threads.extend(self.gmail.list_emails(query=query, limit=limit))
            if self.outlook:
                threads.extend(self.outlook.list_emails(query=query, limit=limit))
            return AgentResponse(
                status="success",
                data={"threads": threads, "user": user_id},
                message="Fetched emails.",
                source_agent=self.name,
            )

        # summarize_thread intent
        if "threadId" in entities and "condition" not in entities:
            thread_id = entities["threadId"]
            body = ""
            if self.gmail:
                try:
                    body = self.gmail.get_email_body(thread_id)
                except Exception:
                    body = ""
            if not body and self.outlook:
                try:
                    body = self.outlook.get_email_body(thread_id)
                except Exception:
                    body = ""
            summary = summarize_email(body)
            return AgentResponse(
                status="success",
                data={"summary": summary, "user": user_id},
                message="Thread summarized.",
                source_agent=self.name,
            )

        # watch_thread intent placeholder
        if "threadId" in entities and "condition" in entities:
            watch_id = f"watch_{entities['threadId']}"
            return AgentResponse(
                status="success",
                data={"watchId": watch_id, "user": user_id},
                message="Thread watch created.",
                source_agent=self.name,
            )

        # default echo behaviour
        return AgentResponse(
            status="success",
            data={"received": entities, "user": user_id},
            message="Inbox processed.",
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
