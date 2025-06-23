from __future__ import annotations

from typing import Any, Dict, List, Optional

from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse

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
