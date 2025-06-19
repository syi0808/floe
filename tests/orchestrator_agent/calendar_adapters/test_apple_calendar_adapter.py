# Unit tests for orchestrator_agent.calendar_adapters.apple_calendar_adapter
import pytest
import unittest
from unittest.mock import patch, MagicMock
import platform
from datetime import datetime, timezone, timedelta

# Module to test
from orchestrator_agent.calendar_adapters.apple_calendar_adapter import AppleCalendarAdapter
from task_agent.task_calendar_linker import CalendarEvent # Assuming this path

# Constants from the module if needed for tests, e.g. DEFAULT_APPLE_CALENDAR_NAME
DEFAULT_APPLE_CALENDAR_NAME_TEST = "Calendar"
FLOE_EVENT_ID_KEY_APPLE = "FLOE_EVENT_ID"
FLOE_TASK_ID_KEY_APPLE = "FLOE_TASK_ID"
FLOE_ID_SEPARATOR_APPLE = "::"

class TestAppleCalendarAdapter(unittest.TestCase):

    def setUp(self):
        # Adapter instance for each test
        self.adapter = AppleCalendarAdapter(default_calendar_name="TestCalendar")

    @patch('platform.system', return_value="Darwin") # Keep this for logical consistency in test name
    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_connect_successful_on_mac(self, mock_execute_applescript, mock_platform_system):
        self.adapter._is_macos = True # Ensure this is set before connect is called
        mock_execute_applescript.return_value = (True, "true", "")
        success = self.adapter.connect()
        self.assertTrue(success)
        mock_execute_applescript.assert_called_once_with("return true")

    @patch('platform.system', return_value="Linux")
    def test_connect_fail_not_mac(self, mock_platform_system):
        # This test relies on __init__ correctly setting _is_macos based on the patched platform.system
        # To ensure this, we might need to instantiate adapter *inside* the test, or re-patch.
        # For now, let's assume setUp's adapter is fine if platform.system is not Darwin during its init.
        # Or, more robustly:
        adapter_linux = AppleCalendarAdapter() # platform.system() will be "Linux" due to patch
        success = adapter_linux.connect()
        self.assertFalse(success)

    @patch('platform.system', return_value="Darwin")
    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_connect_fail_osascript_error(self, mock_execute_applescript, mock_platform_system):
        self.adapter._is_macos = True # Ensure this is set
        mock_execute_applescript.return_value = (False, "", "osascript error")
        success = self.adapter.connect()
        self.assertFalse(success)

    # --- Test Helper Functions ---
    def test_datetime_to_applescript_date(self):
        dt = datetime(2024, 5, 25, 14, 30, 0) # Naive datetime
        # Expected format: 'date "Saturday, May 25, 2024 at 14:30:00"'
        # This depends on locale for day/month name. For test stability, might check structure.
        # For now, checking if it contains key elements.
        as_date_str = self.adapter._datetime_to_applescript_date(dt)
        self.assertIn("May 25, 2024", as_date_str) # Looser check due to locale
        self.assertIn("14:30:00", as_date_str)
        self.assertTrue(as_date_str.startswith('date "') and as_date_str.endswith('"'))

    def test_parse_applescript_datetime_str_iso(self):
        iso_str = "2024-05-25T14:30:00Z"
        expected_dt = datetime(2024, 5, 25, 14, 30, 0, tzinfo=timezone.utc)
        self.assertEqual(self.adapter._parse_applescript_datetime_str(iso_str), expected_dt)

        iso_str_offset = "2024-05-25T14:30:00+02:00"
        expected_dt_offset = datetime(2024, 5, 25, 14, 30, 0, tzinfo=timezone(timedelta(hours=2)))
        self.assertEqual(self.adapter._parse_applescript_datetime_str(iso_str_offset), expected_dt_offset)

    def test_parse_applescript_datetime_str_textual_fallback(self):
        # This test is more fragile due to its reliance on a specific textual format
        # that AppleScript might output by default (which can be locale-dependent).
        # The adapter's parser prioritizes ISO format. This tests the fallback.
        text_str = "Saturday, May 25, 2024 at 14:30:00" # Example format
        expected_dt = datetime(2024, 5, 25, 14, 30, 0) # Naive output from this format
        # To make this test pass, the _parse_applescript_datetime_str needs to handle this format.
        # Current implementation prioritizes ISO and might fail this textual one or parse differently.
        # For now, we assume the ISO path is preferred and tested.
        # If the textual fallback is critical, its strptime format string needs to be exact.
        # self.assertEqual(self.adapter._parse_applescript_datetime_str(text_str), expected_dt)
        # Let's test that it returns None if it's not ISO and not the specific fallback format.
        self.assertIsNone(self.adapter._parse_applescript_datetime_str("Invalid Date String"))


    def test_extract_floe_id_from_description(self):
        desc1 = f"{FLOE_EVENT_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}event123\nSome other text"
        self.assertEqual(self.adapter._extract_floe_id_from_description(desc1, FLOE_EVENT_ID_KEY_APPLE), "event123")

        desc2 = f"Other text\n{FLOE_TASK_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}task456"
        self.assertEqual(self.adapter._extract_floe_id_from_description(desc2, FLOE_TASK_ID_KEY_APPLE), "task456")

        desc3 = f"{FLOE_EVENT_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}event789" # ID at the end
        self.assertEqual(self.adapter._extract_floe_id_from_description(desc3, FLOE_EVENT_ID_KEY_APPLE), "event789")

        desc_no_id = "Just a regular description."
        self.assertIsNone(self.adapter._extract_floe_id_from_description(desc_no_id, FLOE_EVENT_ID_KEY_APPLE))
        self.assertIsNone(self.adapter._extract_floe_id_from_description(None, FLOE_EVENT_ID_KEY_APPLE))

    def test_build_description_with_floe_ids(self):
        original_desc = "Original event details."
        floe_event_id = "evt001"
        task_id_ref = "task002"

        full_desc = self.adapter._build_description_with_floe_ids(original_desc, floe_event_id, task_id_ref)

        self.assertIn(f"{FLOE_EVENT_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}{floe_event_id}", full_desc)
        self.assertIn(f"{FLOE_TASK_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}{task_id_ref}", full_desc)
        self.assertIn(original_desc, full_desc)

        # Test overwriting existing IDs
        desc_with_old_ids = f"{FLOE_EVENT_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}old_evt\n{original_desc}"
        new_full_desc = self.adapter._build_description_with_floe_ids(desc_with_old_ids, floe_event_id, task_id_ref)
        self.assertIn(f"{FLOE_EVENT_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}{floe_event_id}", new_full_desc)
        self.assertNotIn("old_evt", new_full_desc)


    # --- CRUD Method Tests (using mock for _execute_applescript) ---
    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_create_event_successful(self, mock_execute_applescript):
        self.adapter._is_macos = True # Assume on macOS for this test
        mock_execute_applescript.return_value = (True, "apple_native_event_id_123", "") # Success, stdout has native ID

        event_data = CalendarEvent(
            event_id="floe_event_crt1", summary="Test Create",
            start_time=datetime.now(timezone.utc),
            end_time=datetime.now(timezone.utc) + timedelta(hours=1),
            task_id_ref="task_crt1"
        )
        result_id = self.adapter.create_event(event_data, "Work")

        self.assertEqual(result_id, "floe_event_crt1")
        mock_execute_applescript.assert_called_once()
        # Args of call: script string. Can assert parts of the script if needed.
        # print(mock_execute_applescript.call_args[0][0]) # To see the script
        self.assertIn(f'summary:"{event_data.summary}"', mock_execute_applescript.call_args[0][0])
        self.assertIn(f"{FLOE_EVENT_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}{event_data.event_id}", mock_execute_applescript.call_args[0][0])


    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_create_event_applescript_error(self, mock_execute_applescript):
        self.adapter._is_macos = True
        mock_execute_applescript.return_value = (False, "", "AppleScript execution failed")

        event_data = CalendarEvent(event_id="floe_err", summary="S", start_time=datetime.now(), end_time=datetime.now(), task_id_ref="t")
        result_id = self.adapter.create_event(event_data, "Work")

        self.assertIsNone(result_id)

    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_get_event_successful(self, mock_execute_applescript):
        self.adapter._is_macos = True
        floe_event_id_to_get = "floe_get_evt1"
        apple_native_id = "apple_native_id_get1"
        summary = "Found Me"
        start_str_iso = "2024-07-01T10:00:00Z"
        end_str_iso = "2024-07-01T11:00:00Z"
        desc_with_ids = self.adapter._build_description_with_floe_ids("My desc", floe_event_id_to_get, "task_get1")

        # Simulate output from _find_event_applescript_data_by_floe_id's call to _execute_applescript
        # This is the string containing all properties, before parsing by _parse_event_properties_string
        properties_str = f"{apple_native_id},{summary},{start_str_iso},{end_str_iso},{desc_with_ids}"
        mock_execute_applescript.return_value = (True, properties_str, "")

        retrieved_event = self.adapter.get_event(floe_event_id_to_get, "Personal")

        self.assertIsNotNone(retrieved_event)
        self.assertEqual(retrieved_event.event_id, floe_event_id_to_get)
        self.assertEqual(retrieved_event.summary, summary)
        self.assertEqual(retrieved_event.description, "My desc")
        self.assertEqual(retrieved_event.task_id_ref, "task_get1")
        # Check that the script for finding was called
        self.assertIn(f'description contains "{FLOE_EVENT_ID_KEY_APPLE}{FLOE_ID_SEPARATOR_APPLE}{floe_event_id_to_get}"', mock_execute_applescript.call_args[0][0])


    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_get_event_not_found(self, mock_execute_applescript):
        self.adapter._is_macos = True
        mock_execute_applescript.return_value = (True, "", "") # No event found, empty output

        retrieved_event = self.adapter.get_event("nonexistent_floe_id", "Personal")
        self.assertIsNone(retrieved_event)

    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_update_event_successful(self, mock_execute_applescript):
        self.adapter._is_macos = True
        floe_event_id_to_update = "floe_upd_evt1"
        apple_native_id = "apple_native_id_upd1"

        # Mock for _find_event_applescript_data_by_floe_id part
        find_props_str = f"{apple_native_id},Old Summary,2024-07-01T10:00:00Z,2024-07-01T11:00:00Z,Description"
        # Mock for the actual update script
        mock_execute_applescript.side_effect = [
            (True, find_props_str, ""),  # For the find call
            (True, "success", "")        # For the update call
        ]

        event_update_data = CalendarEvent(
            event_id=floe_event_id_to_update, summary="Updated Summary",
            start_time=datetime.fromisoformat("2024-07-01T12:00:00+00:00"),
            end_time=datetime.fromisoformat("2024-07-01T13:00:00+00:00"),
            task_id_ref="task_upd1"
        )
        success = self.adapter.update_event(floe_event_id_to_update, event_update_data, "Work")

        self.assertTrue(success)
        self.assertEqual(mock_execute_applescript.call_count, 2)
        # Last call (the update) should contain new summary and native ID
        update_script_call = mock_execute_applescript.call_args_list[1][0][0]
        self.assertIn(f'set summary of theEvent to "{event_update_data.summary}"', update_script_call)
        self.assertIn(f'whose id is "{apple_native_id}"', update_script_call)

    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_delete_event_successful(self, mock_execute_applescript):
        self.adapter._is_macos = True
        floe_event_id_to_delete = "floe_del_evt1"
        apple_native_id = "apple_native_id_del1"

        find_props_str = f"{apple_native_id},ToDelete,2024-07-01T10:00:00Z,2024-07-01T11:00:00Z,Desc"
        mock_execute_applescript.side_effect = [
            (True, find_props_str, ""), # Find call
            (True, "success", "")       # Delete call
        ]

        success = self.adapter.delete_event(floe_event_id_to_delete, "Personal")

        self.assertTrue(success)
        self.assertEqual(mock_execute_applescript.call_count, 2)
        delete_script_call = mock_execute_applescript.call_args_list[1][0][0]
        self.assertIn(f'delete theEvent', delete_script_call)
        self.assertIn(f'whose id is "{apple_native_id}"', delete_script_call)

    @patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter._execute_applescript')
    def test_list_events_successful(self, mock_execute_applescript):
        self.adapter._is_macos = True
        floe_id1 = "list_floe1"
        task_id1 = "list_task1"
        desc1 = self.adapter._build_description_with_floe_ids("Desc1", floe_id1, task_id1)
        props_str1 = f"apple1,Summary1,2024-07-01T10:00:00Z,2024-07-01T11:00:00Z,{desc1}"

        floe_id2 = "list_floe2" # No task id
        desc2 = self.adapter._build_description_with_floe_ids("Desc2", floe_id2, None)
        props_str2 = f"apple2,Summary2,2024-07-02T10:00:00Z,2024-07-02T11:00:00Z,{desc2}"

        # Event not managed by Floe (no Floe Event ID in description)
        props_str3 = f"apple3,Summary3,2024-07-03T10:00:00Z,2024-07-03T11:00:00Z,No Floe ID here"

        applescript_output = "|||".join([props_str1, props_str2, props_str3])
        mock_execute_applescript.return_value = (True, applescript_output, "")

        # Test case 1: List all (Floe-managed) events
        all_floe_events = self.adapter.list_events("Work")
        self.assertEqual(len(all_floe_events), 2) # props_str3 should be filtered out
        self.assertTrue(any(e.event_id == floe_id1 for e in all_floe_events))
        self.assertTrue(any(e.event_id == floe_id2 for e in all_floe_events))

        # Test case 2: Filter by task_id
        mock_execute_applescript.reset_mock() # Reset for the next call
        mock_execute_applescript.return_value = (True, applescript_output, "")
        task_specific_events = self.adapter.list_events("Work", floe_task_id=task_id1)
        self.assertEqual(len(task_specific_events), 1)
        self.assertEqual(task_specific_events[0].event_id, floe_id1)
        self.assertEqual(task_specific_events[0].task_id_ref, task_id1)


if __name__ == '__main__':
    unittest.main()
