# Unit tests for orchestrator_agent.calendar_adapters.google_calendar_adapter
import pytest
import unittest
from unittest.mock import patch, MagicMock, mock_open

from google.auth.exceptions import RefreshError
from googleapiclient.errors import HttpError

# Module to test
from orchestrator_agent.calendar_adapters.google_calendar_adapter import GoogleCalendarAdapter
from task_agent.task_calendar_linker import CalendarEvent, TaskInput # Assuming this path is correct for tests
from datetime import datetime, timezone, timedelta
import os

# Define constants for testing if not imported from the module itself
DEFAULT_CREDENTIALS_FILE_TEST = "test_credentials.json"
DEFAULT_TOKEN_FILE_TEST = "test_token.json"
FLOE_EVENT_ID_KEY_TEST = "floe_event_id"
FLOE_TASK_ID_KEY_TEST = "floe_task_id"

class TestGoogleCalendarAdapter(unittest.TestCase):

    def setUp(self):
        # Create temporary dummy files for credentials and token for some tests
        # Ensure these paths are handled correctly if adapter uses absolute paths
        self.test_creds_path = DEFAULT_CREDENTIALS_FILE_TEST
        self.test_token_path = DEFAULT_TOKEN_FILE_TEST

        # Basic adapter instance for each test
        # We patch os.path.isabs to prevent attempts to make paths absolute from CWD during test init
        with patch('os.path.isabs', return_value=True):
             self.adapter = GoogleCalendarAdapter(token_file=self.test_token_path, credentials_file=self.test_creds_path)

        # Clean up dummy files if they were created by a previous test run
        if os.path.exists(self.test_creds_path):
            os.remove(self.test_creds_path)
        if os.path.exists(self.test_token_path):
            os.remove(self.test_token_path)

    def tearDown(self):
        # Clean up dummy files after each test
        if os.path.exists(self.test_creds_path):
            os.remove(self.test_creds_path)
        if os.path.exists(self.test_token_path):
            os.remove(self.test_token_path)

    @patch('orchestrator_agent.calendar_adapters.google_calendar_adapter.build')
    @patch('orchestrator_agent.calendar_adapters.google_calendar_adapter.Credentials')
    @patch('os.path.exists')
    def test_connect_successful_with_existing_token(self, mock_path_exists, MockCredentials, mock_build):
        # Arrange
        mock_path_exists.return_value = True # Token file exists
        mock_creds = MagicMock()
        mock_creds.valid = True
        MockCredentials.from_authorized_user_file.return_value = mock_creds

        mock_service = MagicMock()
        mock_build.return_value = mock_service

        # Act
        success = self.adapter.connect()

        # Assert
        self.assertTrue(success)
        self.assertEqual(self.adapter.service, mock_service)
        MockCredentials.from_authorized_user_file.assert_called_once_with(self.test_token_path, self.adapter.SCOPES)
        mock_build.assert_called_once_with("calendar", "v3", credentials=mock_creds)

    @patch('orchestrator_agent.calendar_adapters.google_calendar_adapter.build')
    @patch('orchestrator_agent.calendar_adapters.google_calendar_adapter.InstalledAppFlow')
    @patch('orchestrator_agent.calendar_adapters.google_calendar_adapter.Credentials')
    @patch('os.path.exists')
    @patch('builtins.open', new_callable=mock_open) # Mock open for saving token
    def test_connect_successful_oauth_flow(self, mock_file_open, mock_path_exists, MockCredentials, MockInstalledAppFlow, mock_build):
        # Arrange
        mock_path_exists.side_effect = lambda path: path == self.test_creds_path # Token file doesn't exist, creds file does

        # Simulate no initial valid creds or expired creds
        mock_initial_creds = MagicMock()
        mock_initial_creds.valid = False
        mock_initial_creds.expired = True # or False, depends on the path you want to test
        mock_initial_creds.refresh_token = None # No refresh token, forcing full flow
        MockCredentials.from_authorized_user_file.return_value = mock_initial_creds # Simulate loading invalid token

        mock_flow = MagicMock()
        MockInstalledAppFlow.from_client_secrets_file.return_value = mock_flow

        mock_final_creds = MagicMock()
        mock_final_creds.valid = True
        mock_flow.run_local_server.return_value = mock_final_creds

        mock_service = MagicMock()
        mock_build.return_value = mock_service

        # Act
        success = self.adapter.connect()

        # Assert
        self.assertTrue(success)
        self.assertEqual(self.adapter.service, mock_service)
        MockInstalledAppFlow.from_client_secrets_file.assert_called_once_with(self.test_creds_path, self.adapter.SCOPES)
        mock_flow.run_local_server.assert_called_once_with(port=0)
        mock_file_open.assert_called_once_with(self.test_token_path, "w") # Check token saving
        mock_build.assert_called_once_with("calendar", "v3", credentials=mock_final_creds)

    @patch('os.path.exists')
    def test_connect_credentials_file_not_found(self, mock_path_exists):
        # Arrange
        mock_path_exists.side_effect = lambda path: False # Neither token nor creds file exists

        # Act
        success = self.adapter.connect()

        # Assert
        self.assertFalse(success)
        self.assertIsNone(self.adapter.service)

    @patch('orchestrator_agent.calendar_adapters.google_calendar_adapter.Credentials')
    @patch('os.path.exists')
    def test_connect_refresh_error(self, mock_path_exists, MockCredentials):
        # Arrange
        mock_path_exists.return_value = True # Token file exists
        mock_creds = MagicMock()
        mock_creds.valid = False
        mock_creds.expired = True
        mock_creds.refresh_token = "dummy_refresh_token"
        mock_creds.refresh.side_effect = RefreshError("Refresh failed")
        MockCredentials.from_authorized_user_file.return_value = mock_creds

        # Need to mock the subsequent check for credentials file to avoid FileNotFoundError
        # if refresh fails and it tries to do a full OAuth flow.
        # For this test, we want to focus on RefreshError leading to connection failure.
        # So, assume credentials file does not exist after refresh fails to simplify.
        mock_path_exists.side_effect = lambda path: path == self.test_token_path # Only token exists

        # Act
        success = self.adapter.connect()

        # Assert
        self.assertFalse(success) # Should fail if refresh fails and no credentials.json
        self.assertIsNone(self.adapter.service)

    def test_calendar_event_to_google_event_body(self):
        # Arrange
        floe_event_id = "floe-evt-123"
        floe_task_id = "floe-task-456"
        start_dt = datetime(2024, 1, 1, 10, 0, 0, tzinfo=timezone.utc)
        end_dt = datetime(2024, 1, 1, 11, 0, 0, tzinfo=timezone.utc)

        event_data = CalendarEvent(
            event_id=floe_event_id,
            summary="Test Summary",
            start_time=start_dt,
            end_time=end_dt,
            description="Test Description",
            task_id_ref=floe_task_id
        )
        # Act
        google_body = self.adapter._calendar_event_to_google_event_body(event_data)

        # Assert
        self.assertEqual(google_body["summary"], event_data.summary)
        self.assertEqual(google_body["description"], event_data.description)
        self.assertEqual(google_body["start"]["dateTime"], start_dt.isoformat())
        self.assertEqual(google_body["end"]["dateTime"], end_dt.isoformat())
        self.assertEqual(google_body["extendedProperties"]["private"][FLOE_EVENT_ID_KEY_TEST], floe_event_id)
        self.assertEqual(google_body["extendedProperties"]["private"][FLOE_TASK_ID_KEY_TEST], floe_task_id)

    def test_google_event_to_calendar_event_conversion(self):
        # Arrange
        gcal_event_id = "gcal-evt-789"
        floe_event_id = "floe-evt-123"
        floe_task_id = "floe-task-456"
        start_iso = "2024-01-01T10:00:00Z"
        end_iso = "2024-01-01T11:00:00Z"

        gcal_event_data = {
            "id": gcal_event_id,
            "summary": "Google Event Summary",
            "description": "Google Event Description",
            "start": {"dateTime": start_iso},
            "end": {"dateTime": end_iso},
            "extendedProperties": {
                "private": {
                    FLOE_EVENT_ID_KEY_TEST: floe_event_id,
                    FLOE_TASK_ID_KEY_TEST: floe_task_id
                }
            }
        }
        # Act
        calendar_event = self.adapter._google_event_to_calendar_event(gcal_event_data)

        # Assert
        self.assertIsNotNone(calendar_event)
        self.assertEqual(calendar_event.event_id, floe_event_id)
        self.assertEqual(calendar_event.summary, gcal_event_data["summary"])
        self.assertEqual(calendar_event.description, gcal_event_data["description"])
        self.assertEqual(calendar_event.start_time, datetime.fromisoformat(start_iso.replace("Z", "+00:00")))
        self.assertEqual(calendar_event.end_time, datetime.fromisoformat(end_iso.replace("Z", "+00:00")))
        self.assertEqual(calendar_event.task_id_ref, floe_task_id)

    def test_google_event_to_calendar_event_conversion_no_floe_id(self):
        # Test case where floe_event_id is missing from extended properties, should use gcal_id
        gcal_event_id = "gcal-evt-unique-789"
        start_iso = "2024-01-01T10:00:00Z"
        end_iso = "2024-01-01T11:00:00Z"
        gcal_event_data = {
            "id": gcal_event_id, "summary": "Event without Floe ID",
            "start": {"dateTime": start_iso}, "end": {"dateTime": end_iso}
        }
        calendar_event = self.adapter._google_event_to_calendar_event(gcal_event_data)
        self.assertEqual(calendar_event.event_id, gcal_event_id) # Falls back to gcal native ID

    # --- CRUD method tests ---
    def _setup_mock_service_for_adapter(self):
        """Helper to inject a mock service into the adapter instance for CRUD tests."""
        self.adapter.service = MagicMock()
        return self.adapter.service

    def test_create_event_successful(self):
        mock_service = self._setup_mock_service_for_adapter()

        mock_insert_request = MagicMock()
        mock_service.events().insert.return_value = mock_insert_request
        mock_insert_request.execute.return_value = {"id": "gcal_new_id"}

        event_data = CalendarEvent(event_id="floe1", summary="S", start_time=datetime.now(timezone.utc),
                                   end_time=datetime.now(timezone.utc) + timedelta(hours=1), task_id_ref="task1")

        result_id = self.adapter.create_event(event_data, "primary")

        self.assertEqual(result_id, "floe1")
        mock_service.events().insert.assert_called_once_with(calendarId="primary", body=unittest.mock.ANY)
        mock_insert_request.execute.assert_called_once()


    def test_create_event_api_error(self):
        mock_service = self._setup_mock_service_for_adapter()
        mock_service.events().insert().execute.side_effect = HttpError(MagicMock(status=500), b"Error")

        event_data = CalendarEvent(event_id="floe1", summary="S", start_time=datetime.now(timezone.utc),
                                   end_time=datetime.now(timezone.utc) + timedelta(hours=1), task_id_ref="task1")

        result_id = self.adapter.create_event(event_data, "primary")
        self.assertIsNone(result_id)

    def test_get_event_successful(self):
        mock_service = self._setup_mock_service_for_adapter()
        floe_event_id = "floe-get-1"
        gcal_event_data = {
            "id": "gcal-get-1", "summary": "Found Event",
            "start": {"dateTime": "2024-01-01T10:00:00Z"}, "end": {"dateTime": "2024-01-01T11:00:00Z"},
            "extendedProperties": {"private": {FLOE_EVENT_ID_KEY_TEST: floe_event_id}}
        }

        mock_list_request = MagicMock()
        mock_service.events().list.return_value = mock_list_request
        mock_list_request.execute.return_value = {"items": [gcal_event_data]}

        retrieved_event = self.adapter.get_event(floe_event_id, "primary")

        self.assertIsNotNone(retrieved_event)
        self.assertEqual(retrieved_event.event_id, floe_event_id)
        self.assertEqual(retrieved_event.summary, "Found Event")
        mock_service.events().list.assert_called_once_with(
            calendarId="primary", privateExtendedProperty=f"{FLOE_EVENT_ID_KEY_TEST}={floe_event_id}", maxResults=1
        )
        mock_list_request.execute.assert_called_once()


    def test_get_event_not_found(self):
        mock_service = self._setup_mock_service_for_adapter()
        mock_list_request = MagicMock()
        mock_service.events().list.return_value = mock_list_request
        mock_list_request.execute.return_value = {"items": []} # No items found

        retrieved_event = self.adapter.get_event("nonexistent-floe-id", "primary")
        self.assertIsNone(retrieved_event)

    def test_update_event_successful(self):
        mock_service = self._setup_mock_service_for_adapter()
        floe_event_id = "floe-update-1"
        gcal_id_to_update = "gcal-update-1"

        # Mock the find operation first
        gcal_event_data_found = {
            "id": gcal_id_to_update, "summary": "Old Summary",
            "start": {"dateTime": "2024-01-01T10:00:00Z"}, "end": {"dateTime": "2024-01-01T11:00:00Z"},
            "extendedProperties": {"private": {FLOE_EVENT_ID_KEY_TEST: floe_event_id}}
        }
        # This list().execute() is for the find operation (_find_gcal_event_by_floe_id)
        mock_find_list_request = MagicMock()
        mock_service.events().list.return_value = mock_find_list_request
        mock_find_list_request.execute.return_value = {"items": [gcal_event_data_found]}

        # Mock the update operation itself
        mock_update_request = MagicMock()
        mock_service.events().update.return_value = mock_update_request
        mock_update_request.execute.return_value = {"id": gcal_id_to_update, "summary": "Updated Summary"}

        event_update_data = CalendarEvent(
            event_id=floe_event_id, summary="Updated Summary",
            start_time=datetime.fromisoformat("2024-01-01T10:00:00+00:00"),
            end_time=datetime.fromisoformat("2024-01-01T11:00:00+00:00"),
            task_id_ref="task-update-1"
        )
        success = self.adapter.update_event(floe_event_id, event_update_data, "primary")

        self.assertTrue(success)
        # Check find call was made
        mock_service.events().list.assert_called_with(
            calendarId="primary", privateExtendedProperty=f"{FLOE_EVENT_ID_KEY_TEST}={floe_event_id}", maxResults=1
        )
        mock_find_list_request.execute.assert_called_once()
        # Check update call
        mock_service.events().update.assert_called_once_with(calendarId="primary", eventId=gcal_id_to_update, body=unittest.mock.ANY)
        mock_update_request.execute.assert_called_once()


    def test_delete_event_successful(self):
        mock_service = self._setup_mock_service_for_adapter()
        floe_event_id = "floe-delete-1"
        gcal_id_to_delete = "gcal-delete-1"

        # Mock for the _find_gcal_event_by_floe_id call
        gcal_event_data_found = {
            "id": gcal_id_to_delete, "summary": "To Be Deleted",
            "start": {"dateTime": "2024-01-01T10:00:00Z"}, "end": {"dateTime": "2024-01-01T11:00:00Z"},
            "extendedProperties": {"private": {FLOE_EVENT_ID_KEY_TEST: floe_event_id}}
        }
        # This list().execute() is for the find operation
        mock_find_list_request = mock_service.events().list.return_value
        mock_find_list_request.execute.return_value = {"items": [gcal_event_data_found]}

        # This is for the actual delete operation
        mock_delete_request = mock_service.events().delete.return_value
        mock_delete_request.execute.return_value = {}

        success = self.adapter.delete_event(floe_event_id, "primary")

        self.assertTrue(success)
        # Check find call
        mock_service.events().list.assert_called_with(
            calendarId="primary", privateExtendedProperty=f"{FLOE_EVENT_ID_KEY_TEST}={floe_event_id}", maxResults=1
        )
        # Check delete call
        mock_service.events().delete.assert_called_once_with(calendarId="primary", eventId=gcal_id_to_delete)
        mock_delete_request.execute.assert_called_once()


    def test_list_events_successful(self):
        mock_service = self._setup_mock_service_for_adapter()
        floe_event_id1 = "floe-list-1"
        floe_event_id2 = "floe-list-2"
        gcal_events_data = [
            {"id": "g1", "summary": "Event 1", "start": {"dateTime": "2024-01-01T10:00:00Z"}, "end": {"dateTime": "2024-01-01T11:00:00Z"}, "extendedProperties": {"private": {FLOE_EVENT_ID_KEY_TEST: floe_event_id1, FLOE_TASK_ID_KEY_TEST: "task1"}}},
            {"id": "g2", "summary": "Event 2", "start": {"dateTime": "2024-01-02T10:00:00Z"}, "end": {"dateTime": "2024-01-02T11:00:00Z"}, "extendedProperties": {"private": {FLOE_EVENT_ID_KEY_TEST: floe_event_id2, FLOE_TASK_ID_KEY_TEST: "task2"}}},
            {"id": "g3", "summary": "Non-Floe Event", "start": {"dateTime": "2024-01-03T10:00:00Z"}, "end": {"dateTime": "2024-01-03T11:00:00Z"}} # No Floe ID
        ]
        mock_service.events().list().execute.return_value = {"items": gcal_events_data}

        listed_calendar_events = self.adapter.list_events("primary", floe_task_id="task1")

        self.assertEqual(len(listed_calendar_events), 1)
        self.assertEqual(listed_calendar_events[0].event_id, floe_event_id1)
        self.assertEqual(listed_calendar_events[0].task_id_ref, "task1")

        # Test listing without task_id filter, should only return floe events
        mock_service.events().list.reset_mock() # Reset mock for the next call
        mock_service.events().list().execute.return_value = {"items": gcal_events_data}
        all_floe_events = self.adapter.list_events("primary")
        self.assertEqual(len(all_floe_events), 2) # Event g3 should be filtered out
        self.assertTrue(any(e.event_id == floe_event_id1 for e in all_floe_events))
        self.assertTrue(any(e.event_id == floe_event_id2 for e in all_floe_events))


if __name__ == '__main__':
    unittest.main()
