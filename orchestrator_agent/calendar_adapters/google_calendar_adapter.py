# orchestrator_agent/calendar_adapters/google_calendar_adapter.py
import os
import json
from datetime import datetime, timezone, timedelta
from typing import Optional, List, Any, Dict
import uuid

from google.auth.transport.requests import Request
from google.oauth2.credentials import Credentials
from google.auth.exceptions import RefreshError
from google_auth_oauthlib.flow import InstalledAppFlow
from googleapiclient.discovery import build
from googleapiclient.errors import HttpError

# CalendarEvent is imported from .base_adapter below
from .base_adapter import CalendarAdapter
from task_agent.task_calendar_linker import CalendarEvent

# Default paths, assuming execution from project root or paths are absolute
DEFAULT_CREDENTIALS_FILE = "credentials.json"
DEFAULT_TOKEN_FILE = "token.json"

class GoogleCalendarAdapter(CalendarAdapter):
    SCOPES = ["https://www.googleapis.com/auth/calendar.events"]
    FLOE_EVENT_ID_KEY = "floe_event_id"
    FLOE_TASK_ID_KEY = "floe_task_id"

    def __init__(self, token_file: str = DEFAULT_TOKEN_FILE, credentials_file: str = DEFAULT_CREDENTIALS_FILE):
        self.service: Optional[Any] = None
        self.token_file = token_file
        self.credentials_file = credentials_file

        # Ensure paths are absolute for robustness if adapter is called from various CWDs
        # If files are always expected at project root, this needs to be relative to project root.
        # For now, let's assume if not absolute, they are in CWD or need to be made absolute by caller.
        # The test setUp handles this by creating files in CWD for testing.
        # Original code in __main__ constructed absolute paths from project root.
        # Let's keep it simple for now and assume paths passed are usable as is, or made absolute by caller.
        # if not os.path.isabs(self.token_file):
        #     self.token_file = os.path.join(os.getcwd(), self.token_file)
        # if not os.path.isabs(self.credentials_file):
        #     self.credentials_file = os.path.join(os.getcwd(), self.credentials_file)

    def connect(self) -> bool:
        creds = None
        if os.path.exists(self.token_file):
            try:
                creds = Credentials.from_authorized_user_file(self.token_file, self.SCOPES)
            except Exception as e:
                print(f"Error loading token file '{self.token_file}': {e}")
                creds = None

        if not creds or not creds.valid:
            if creds and creds.expired and creds.refresh_token:
                try:
                    print(f"Refreshing expired credentials from {self.token_file}...")
                    creds.refresh(Request())
                    print("Credentials refreshed successfully.")
                except RefreshError as e:
                    print(f"Error refreshing credentials: {e}")
                    creds = None
                except Exception as e:
                    print(f"An unexpected error occurred during credential refresh: {e}")
                    creds = None

            if not creds or not creds.valid:
                print(f"No valid credentials in {self.token_file}, attempting new authorization flow...")
                if not os.path.exists(self.credentials_file):
                    print(f"Error: Credentials file '{self.credentials_file}' not found.")
                    return False
                try:
                    flow = InstalledAppFlow.from_client_secrets_file(self.credentials_file, self.SCOPES)
                    creds = flow.run_local_server(port=0)
                    print("Authorization successful.")
                except Exception as e:
                    print(f"Error during authorization flow: {e}")
                    return False

            if creds:
                try:
                    with open(self.token_file, "w") as token:
                        token.write(creds.to_json())
                    print(f"Credentials saved to '{self.token_file}'.")
                except Exception as e:
                    print(f"Error saving token to '{self.token_file}': {e}")

        if creds and creds.valid:
            try:
                self.service = build("calendar", "v3", credentials=creds)
                print("Google Calendar service built successfully.")
                return True
            except Exception as e:
                print(f"Error building Google Calendar service: {e}")
                self.service = None
                return False
        else:
            print("Failed to obtain valid credentials.")
            self.service = None
            return False

    def _datetime_from_google_format(self, google_datetime_obj: Optional[Dict[str, str]]) -> Optional[datetime]:
        if not google_datetime_obj: return None
        dt_str = google_datetime_obj.get('dateTime') or google_datetime_obj.get('date')
        if not dt_str: return None
        try:
            if 'T' in dt_str: # Datetime string
                if dt_str.endswith("Z"): return datetime.fromisoformat(dt_str.replace("Z", "+00:00"))
                return datetime.fromisoformat(dt_str)
            else: # Date string
                return datetime.strptime(dt_str, "%Y-%m-%d").replace(tzinfo=timezone.utc)
        except ValueError as e:
            print(f"Error parsing date string '{dt_str}': {e}")
            return None

    def _google_event_to_calendar_event(self, gcal_event: Dict[str, Any]) -> Optional[CalendarEvent]:
        if not gcal_event: return None
        start_time = self._datetime_from_google_format(gcal_event.get("start"))
        end_time = self._datetime_from_google_format(gcal_event.get("end"))
        if not start_time or not end_time: return None

        extended_props = gcal_event.get("extendedProperties", {}).get("private", {})
        floe_event_id = extended_props.get(self.FLOE_EVENT_ID_KEY, gcal_event.get("id"))
        floe_task_id_ref = extended_props.get(self.FLOE_TASK_ID_KEY)

        return CalendarEvent(
            event_id=floe_event_id,
            summary=gcal_event.get("summary", "No Title"),
            start_time=start_time,
            end_time=end_time,
            description=gcal_event.get("description"),
            task_id_ref=floe_task_id_ref
        )

    def _calendar_event_to_google_event_body(self, event_data: CalendarEvent) -> Dict[str, Any]:
        body = {
            "summary": event_data.summary,
            "description": event_data.description,
            "start": {"dateTime": event_data.start_time.isoformat()},
            "end": {"dateTime": event_data.end_time.isoformat()},
            "extendedProperties": {"private": {self.FLOE_EVENT_ID_KEY: event_data.event_id}}
        }
        if event_data.start_time.tzinfo: body["start"]["timeZone"] = str(event_data.start_time.tzinfo)
        if event_data.end_time.tzinfo: body["end"]["timeZone"] = str(event_data.end_time.tzinfo)
        if event_data.task_id_ref: body["extendedProperties"]["private"][self.FLOE_TASK_ID_KEY] = event_data.task_id_ref
        return body

    def _find_gcal_event_by_floe_id(self, floe_event_id: str, calendar_target: Optional[str]) -> Optional[Dict[str, Any]]:
        if not self.service: return None
        calendar_id = calendar_target or "primary"
        query = f"{self.FLOE_EVENT_ID_KEY}={floe_event_id}"
        try:
            events_result = self.service.events().list(
                calendarId=calendar_id, privateExtendedProperty=query, maxResults=1
            ).execute()
            return events_result.get("items", [])[0] if events_result.get("items") else None
        except HttpError as e:
            print(f"HTTP error finding event by floe_event_id '{floe_event_id}': {e}")
            return None

    def create_event(self, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> Optional[str]:
        if not self.service: return None
        if not event_data.event_id: return None # Must have our internal ID
        calendar_id = calendar_target or "primary"
        event_body = self._calendar_event_to_google_event_body(event_data)
        try:
            self.service.events().insert(calendarId=calendar_id, body=event_body).execute()
            return event_data.event_id
        except HttpError as e:
            print(f"HTTP error creating event (Floe ID: {event_data.event_id}): {e}")
            return None

    def get_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> Optional[CalendarEvent]:
        gcal_event = self._find_gcal_event_by_floe_id(floe_event_id, calendar_target)
        return self._google_event_to_calendar_event(gcal_event) if gcal_event else None

    def update_event(self, floe_event_id: str, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> bool:
        if not self.service: return False
        if event_data.event_id != floe_event_id: return False # ID mismatch

        gcal_event_to_update = self._find_gcal_event_by_floe_id(floe_event_id, calendar_target)
        if not gcal_event_to_update: return False

        gcal_id = gcal_event_to_update.get("id")
        if not gcal_id: return False

        calendar_id = calendar_target or "primary"
        event_body = self._calendar_event_to_google_event_body(event_data)
        try:
            self.service.events().update(calendarId=calendar_id, eventId=gcal_id, body=event_body).execute()
            return True
        except HttpError as e:
            print(f"HTTP error updating event (Floe ID: {floe_event_id}): {e}")
            return False

    def delete_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> bool:
        if not self.service: return False
        gcal_event_to_delete = self._find_gcal_event_by_floe_id(floe_event_id, calendar_target)
        if not gcal_event_to_delete: return False

        gcal_id = gcal_event_to_delete.get("id")
        if not gcal_id: return False

        calendar_id = calendar_target or "primary"
        try:
            self.service.events().delete(calendarId=calendar_id, eventId=gcal_id).execute()
            return True
        except HttpError as e:
            if e.resp.status in [404, 410]: return True # Idempotent
            print(f"HTTP error deleting event (Floe ID: {floe_event_id}): {e}")
            return False

    def list_events(self, calendar_target: Optional[str] = None,
                    time_min: Optional[datetime] = None, time_max: Optional[datetime] = None,
                    floe_task_id: Optional[str] = None) -> List[CalendarEvent]:
        if not self.service: return []
        calendar_id = calendar_target or "primary"
        query_params: Dict[str, Any] = {"calendarId": calendar_id, "singleEvents": True, "orderBy": "startTime"}
        if time_min: query_params["timeMin"] = time_min.isoformat()
        if time_max: query_params["timeMax"] = time_max.isoformat()

        private_props_filters = []
        if floe_task_id: private_props_filters.append(f"{self.FLOE_TASK_ID_KEY}={floe_task_id}")
        # To list only Floe-managed events, we'd ideally filter for existence of self.FLOE_EVENT_ID_KEY.
        # Google's API doesn't support "exists" for extended props. Client-side filtering is needed.
        if private_props_filters: query_params["privateExtendedProperty"] = ",".join(private_props_filters)

        try:
            events_result = self.service.events().list(**query_params).execute()
            gcal_items = events_result.get("items", [])
            calendar_events: List[CalendarEvent] = []
            for item in gcal_items:
                ce = self._google_event_to_calendar_event(item)
                if ce:
                    extended_props = item.get("extendedProperties", {}).get("private", {})
                    # It must be a Floe-managed event (has our main event ID key)
                    if self.FLOE_EVENT_ID_KEY in extended_props:
                        if floe_task_id: # If a task filter is active
                            if ce.task_id_ref == floe_task_id: # And it matches the task filter
                                calendar_events.append(ce)
                        else: # No task filter, so add if it's a Floe event
                            calendar_events.append(ce)
            return calendar_events
        except HttpError as e:
            print(f"HTTP error listing events: {e}")
            return []

# --- Main block (Updated) ---
if __name__ == "__main__":
    print("Google Calendar Adapter - Direct Execution Test")
    project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
    abs_token_file = os.path.join(project_root, DEFAULT_TOKEN_FILE)
    abs_credentials_file = os.path.join(project_root, DEFAULT_CREDENTIALS_FILE)

    print(f"Token file: {abs_token_file}\nCredentials file: {abs_credentials_file}")
    # Placeholder creation logic for credentials.json can be added here if needed for first run

    adapter = GoogleCalendarAdapter(token_file=abs_token_file, credentials_file=abs_credentials_file)
    if adapter.connect():
        print("Successfully connected.")
        # Basic test: List upcoming 5 events if any
        try:
            print("\nListing up to 5 upcoming Floe-managed events:")
            now = datetime.now(timezone.utc)
            later = now + timedelta(days=7)
            events = adapter.list_events(time_min=now, time_max=later)
            if not events:
                print("No Floe-managed events found in the next 7 days.")
            for i, event in enumerate(events[:5]):
                print(f"  {i+1}. {event.summary} (Floe ID: {event.event_id}) @ {event.start_time}")
        except Exception as e:
            print(f"Error during example list_events: {e}")
    else:
        print("Failed to connect.")
    print("\n--- GoogleCalendarAdapter Test Run Complete ---")
