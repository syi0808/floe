from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any, Dict, List, Optional
import datetime
import requests


class AbstractEmailConnector(ABC):
    """Interface for email service connectors."""

    @abstractmethod
    def list_emails(self, query: str = "", limit: int = 10) -> List[Dict[str, Any]]:
        """Return a list of email metadata dictionaries."""

    @abstractmethod
    def get_email_body(self, email_id: str) -> str:
        """Fetch the plain text body for the given email."""

    @abstractmethod
    def get_attachments(self, email_id: str) -> List[Dict[str, Any]]:
        """Return attachment info dictionaries for an email."""


class GmailConnector(AbstractEmailConnector):
    """Simple Gmail API wrapper using HTTP requests."""

    def __init__(
        self,
        access_token: str | None = None,
        refresh_token: str | None = None,
        client_id: str | None = None,
        client_secret: str | None = None,
    ) -> None:
        self.access_token = access_token
        self.refresh_token = refresh_token
        self.client_id = client_id
        self.client_secret = client_secret
        self.base_url = "https://gmail.googleapis.com/gmail/v1/users/me"
        self.token_expires_at: Optional[datetime.datetime] = None

    def _refresh_token(self) -> None:
        if not all([self.refresh_token, self.client_id, self.client_secret]):
            return
        data = {
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "refresh_token": self.refresh_token,
            "grant_type": "refresh_token",
        }
        resp = requests.post("https://oauth2.googleapis.com/token", data=data)
        resp.raise_for_status()
        token_data = resp.json()
        if "access_token" not in token_data:
            raise ValueError("Token refresh response missing access_token")
        self.access_token = token_data["access_token"]
        if "expires_in" in token_data:
            self.token_expires_at = datetime.datetime.utcnow() + datetime.timedelta(seconds=int(token_data["expires_in"]))
    def _authorized_request(
        self,
        method: str,
        endpoint: str,
        params: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        if not self.access_token or (
            self.token_expires_at and self.token_expires_at <= datetime.datetime.utcnow()
        ):
            self._refresh_token()
        headers = {"Authorization": f"Bearer {self.access_token}"}
        url = f"{self.base_url}/{endpoint}"
        if method.lower() == "get":
            resp = requests.get(url, headers=headers, params=params)
        else:
            resp = requests.request(method, url, headers=headers, params=params)
        if resp.status_code == 401:
            self._refresh_token()
            headers["Authorization"] = f"Bearer {self.access_token}"
            if method.lower() == "get":
                resp = requests.get(url, headers=headers, params=params)
            else:
                resp = requests.request(method, url, headers=headers, params=params)
        resp.raise_for_status()
        return resp.json()

    def _get(self, endpoint: str, params: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        return self._authorized_request("get", endpoint, params=params)

    def list_emails(self, query: str = "", limit: int = 10) -> List[Dict[str, Any]]:
        data = self._get("messages", params={"q": query, "maxResults": limit})
        return data.get("messages", [])

    def get_email_body(self, email_id: str) -> str:
        data = self._get(f"messages/{email_id}", params={"format": "full"})
        parts = data.get("payload", {}).get("parts", [])
        if parts:
            import base64

            body_data = parts[0].get("body", {}).get("data", "")
            try:
                return base64.urlsafe_b64decode(body_data).decode("utf-8")
            except Exception:
                return ""
        return data.get("snippet", "")

    def get_attachments(self, email_id: str) -> List[Dict[str, Any]]:
        data = self._get(f"messages/{email_id}", params={"format": "full"})
        attachments: List[Dict[str, Any]] = []
        for part in data.get("payload", {}).get("parts", []):
            if part.get("filename"):
                attachments.append({"filename": part.get("filename"), "mime_type": part.get("mimeType")})
        return attachments


class OutlookConnector(AbstractEmailConnector):
    """Microsoft Outlook connector using Graph API."""

    def __init__(
        self,
        access_token: str | None = None,
        refresh_token: str | None = None,
        client_id: str | None = None,
        client_secret: str | None = None,
        tenant: str = "common",
    ) -> None:
        self.access_token = access_token
        self.refresh_token = refresh_token
        self.client_id = client_id
        self.client_secret = client_secret
        self.tenant = tenant
        self.base_url = "https://graph.microsoft.com/v1.0/me"
        self.token_expires_at: Optional[datetime.datetime] = None

    def _refresh_token(self) -> None:
        if not all([self.refresh_token, self.client_id, self.client_secret]):
            return
        data = {
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "refresh_token": self.refresh_token,
            "grant_type": "refresh_token",
            "scope": "https://graph.microsoft.com/.default",
        }
        token_url = f"https://login.microsoftonline.com/{self.tenant}/oauth2/v2.0/token"
        resp = requests.post(token_url, data=data)
        resp.raise_for_status()
        token_data = resp.json()
        self.access_token = token_data.get("access_token")
        if "expires_in" in token_data:
            self.token_expires_at = datetime.datetime.utcnow() + datetime.timedelta(seconds=int(token_data["expires_in"]))

    def _authorized_request(
        self,
        method: str,
        endpoint: str,
        params: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        if not self.access_token or (
            self.token_expires_at and self.token_expires_at <= datetime.datetime.utcnow()
        ):
            self._refresh_token()
        headers = {"Authorization": f"Bearer {self.access_token}"}
        url = f"{self.base_url}/{endpoint}"
        if method.lower() == "get":
            resp = requests.get(url, headers=headers, params=params)
        else:
            resp = requests.request(method, url, headers=headers, params=params)
        if resp.status_code == 401:
            self._refresh_token()
            headers["Authorization"] = f"Bearer {self.access_token}"
            if method.lower() == "get":
                resp = requests.get(url, headers=headers, params=params)
            else:
                resp = requests.request(method, url, headers=headers, params=params)
        resp.raise_for_status()
        return resp.json()

    def _get(self, endpoint: str, params: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        return self._authorized_request("get", endpoint, params=params)

    def list_emails(self, query: str = "", limit: int = 10) -> List[Dict[str, Any]]:
        params = {"$top": limit}
        if query:
            params["$search"] = query
        data = self._get("messages", params=params)
        return data.get("value", [])

    def get_email_body(self, email_id: str) -> str:
        data = self._get(f"messages/{email_id}")
        return data.get("body", {}).get("content", "")

    def get_attachments(self, email_id: str) -> List[Dict[str, Any]]:
        data = self._get(f"messages/{email_id}/attachments")
        return data.get("value", [])
