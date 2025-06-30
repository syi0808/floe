import pytest
import unittest
from unittest.mock import MagicMock, patch
from datetime import datetime, timedelta, timezone
from pydantic import ValidationError
import uuid # For checking UUID format

# Modules to test
from task_agent.task_calendar_linker import (
    TaskInput,
    CalendarEvent,
    create_calendar_event_from_task, # Assuming this is still exported and used
    TaskCalendarLinker
)
from orchestrator_agent.calendar_adapters.base_adapter import CalendarAdapter # For spec in mock

# --- Test Data ---
SAMPLE_START_TIME = datetime.now(timezone.utc) + timedelta(days=1) # Use timezone-aware datetime
VALID_TASK_DATA_DICT = {
    "task_id": "task_001",
    "description": "This is a detailed description.",
    "start_time": SAMPLE_START_TIME,
    "duration_minutes": 60,
    "summary": "Task Summary"
}

# --- Existing Model Tests (largely unchanged but ensure timezone awareness if applicable) ---

class TestTaskInput(unittest.TestCase):
    def test_valid_task_input(self):
        task_input = TaskInput(**VALID_TASK_DATA_DICT)
        self.assertEqual(task_input.task_id, VALID_TASK_DATA_DICT["task_id"])
        # ... other assertions

    def test_task_input_invalid_duration(self):
        with self.assertRaises(ValidationError):
            TaskInput(**{**VALID_TASK_DATA_DICT, "duration_minutes": 0})
    # ... other TaskInput tests from original file


class TestCalendarEvent(unittest.TestCase):
    def test_valid_calendar_event(self):
        start = datetime.now(timezone.utc)
        event = CalendarEvent(
            event_id="evt_001", summary="Valid Event",
            start_time=start, end_time=start + timedelta(hours=1),
            task_id_ref="task_ref_001"
        )
        self.assertEqual(event.event_id, "evt_001")

    def test_calendar_event_end_time_validation(self):
        start = datetime.now(timezone.utc)
        with self.assertRaises(ValidationError): # end_time <= start_time
            CalendarEvent(event_id="evt_002", summary="Invalid Times", start_time=start, end_time=start, task_id_ref="t2")
    # ... other CalendarEvent tests


# --- Updated Tests for create_calendar_event_from_task ---

class TestCreateCalendarEventFromTask(unittest.TestCase):

    def _is_uuid(self, id_string):
        try:
            uuid.UUID(id_string)
            return True
        except ValueError:
            return False

    def test_create_new_event_generates_uuid_event_id(self):
        task_input = TaskInput(**VALID_TASK_DATA_DICT)
        calendar_event = create_calendar_event_from_task(task_input) # No base_event_id

        self.assertIsNotNone(calendar_event.event_id)
        self.assertTrue(self._is_uuid(calendar_event.event_id), "Generated event_id should be a UUID.")
        self.assertEqual(calendar_event.summary, VALID_TASK_DATA_DICT["summary"])
        self.assertEqual(calendar_event.task_id_ref, VALID_TASK_DATA_DICT["task_id"])

    def test_create_event_uses_base_event_id_if_provided(self):
        task_input = TaskInput(**VALID_TASK_DATA_DICT)
        custom_event_id = "my_custom_event_id_123"
        calendar_event = create_calendar_event_from_task(task_input, base_event_id=custom_event_id)

        self.assertEqual(calendar_event.event_id, custom_event_id)
        self.assertEqual(calendar_event.summary, VALID_TASK_DATA_DICT["summary"])

    def test_create_event_summary_derivation(self):
        task_data_no_summary = {**VALID_TASK_DATA_DICT}
        del task_data_no_summary["summary"]
        task_input = TaskInput(**task_data_no_summary)
        calendar_event = create_calendar_event_from_task(task_input)

        expected_summary = task_input.description[:100]
        self.assertEqual(calendar_event.summary, expected_summary)

    def test_create_event_type_error(self):
        with self.assertRaises(TypeError):
            create_calendar_event_from_task({"not_a_task_input_object": True})


# --- New Tests for TaskCalendarLinker ---

class TestTaskCalendarLinker(unittest.TestCase):
    def setUp(self):
        self.mock_adapter = MagicMock(spec=CalendarAdapter)
        self.linker = TaskCalendarLinker(adapter=self.mock_adapter)
        self.sample_task_input = TaskInput(**VALID_TASK_DATA_DICT)
        self.floe_event_id = "test_floe_event_id_123"

    def test_connect_calendar_successful(self):
        self.mock_adapter.connect.return_value = True
        self.assertTrue(self.linker.connect_calendar())
        self.assertTrue(self.linker._connected)
        self.mock_adapter.connect.assert_called_once()

    def test_connect_calendar_failure(self):
        self.mock_adapter.connect.return_value = False
        self.assertFalse(self.linker.connect_calendar())
        self.assertFalse(self.linker._connected)

    def test_operations_fail_if_not_connected(self):
        # Ensure linker is not connected
        self.linker._connected = False
        expected_msg = "Calendar adapter not connected. Call connect_calendar() first."

        with self.assertRaises(RuntimeError) as cm_add:
            self.linker.add_task_to_calendar(self.sample_task_input)
        self.assertEqual(str(cm_add.exception), expected_msg)

        with self.assertRaises(RuntimeError) as cm_get:
            self.linker.get_linked_event(self.floe_event_id)
        self.assertEqual(str(cm_get.exception), expected_msg)

        with self.assertRaises(RuntimeError) as cm_update:
            self.linker.update_linked_event(self.floe_event_id, self.sample_task_input)
        self.assertEqual(str(cm_update.exception), expected_msg)

        with self.assertRaises(RuntimeError) as cm_remove:
            self.linker.remove_task_from_calendar(self.floe_event_id)
        self.assertEqual(str(cm_remove.exception), expected_msg)

        with self.assertRaises(RuntimeError) as cm_list:
            self.linker.list_linked_events()
        self.assertEqual(str(cm_list.exception), expected_msg)


    def test_add_task_to_calendar(self):
        self.linker._connected = True # Assume connected
        # create_calendar_event_from_task will generate a CalendarEvent with a UUID event_id
        # We need to mock what the adapter returns for this event_id.

        # Capture the CalendarEvent passed to adapter.create_event
        # The actual event_id is generated inside create_calendar_event_from_task
        # So we mock adapter.create_event to return the event_id it receives.
        def side_effect_create_event(event_data, calendar_target):
            return event_data.event_id
        self.mock_adapter.create_event.side_effect = side_effect_create_event

        returned_event_id = self.linker.add_task_to_calendar(self.sample_task_input, "primary")

        self.assertIsNotNone(returned_event_id)
        self.assertTrue(uuid.UUID(returned_event_id), "Returned ID should be a UUID string.")
        self.mock_adapter.create_event.assert_called_once()
        # Assert properties of the CalendarEvent passed to adapter.create_event
        call_args = self.mock_adapter.create_event.call_args[0] # Get positional args
        created_calendar_event_arg = call_args[0] # First arg is event_data
        self.assertEqual(created_calendar_event_arg.summary, self.sample_task_input.summary)
        self.assertEqual(created_calendar_event_arg.task_id_ref, self.sample_task_input.task_id)


    def test_get_linked_event(self):
        self.linker._connected = True
        mock_event = CalendarEvent(event_id=self.floe_event_id, summary="S", start_time=datetime.now(), end_time=datetime.now(), task_id_ref="T")
        self.mock_adapter.get_event.return_value = mock_event

        event = self.linker.get_linked_event(self.floe_event_id, "primary")

        self.assertEqual(event, mock_event)
        self.mock_adapter.get_event.assert_called_once_with(self.floe_event_id, "primary")

    def test_update_linked_event(self):
        self.linker._connected = True
        self.mock_adapter.update_event.return_value = True

        # create_calendar_event_from_task will be called with base_event_id=self.floe_event_id
        # The CalendarEvent passed to adapter.update_event should have this ID.
        success = self.linker.update_linked_event(self.floe_event_id, self.sample_task_input, "primary")

        self.assertTrue(success)
        self.mock_adapter.update_event.assert_called_once()
        call_args = self.mock_adapter.update_event.call_args[0]
        updated_calendar_event_arg = call_args[1] # Second arg is event_data
        self.assertEqual(call_args[0], self.floe_event_id) # First arg is floe_event_id
        self.assertEqual(updated_calendar_event_arg.event_id, self.floe_event_id) # Ensure it's the same
        self.assertEqual(updated_calendar_event_arg.summary, self.sample_task_input.summary)


    def test_remove_task_from_calendar(self):
        self.linker._connected = True
        self.mock_adapter.delete_event.return_value = True

        success = self.linker.remove_task_from_calendar(self.floe_event_id, "primary")

        self.assertTrue(success)
        self.mock_adapter.delete_event.assert_called_once_with(self.floe_event_id, "primary")

    def test_list_linked_events(self):
        self.linker._connected = True
        mock_events = [CalendarEvent(event_id="e1", summary="S1", start_time=datetime.now(), end_time=datetime.now(), task_id_ref="T1")]
        self.mock_adapter.list_events.return_value = mock_events

        start_filter = datetime.now() - timedelta(days=1)
        end_filter = datetime.now() + timedelta(days=1)

        events = self.linker.list_linked_events("primary", start_filter, end_filter, "task123")

        self.assertEqual(events, mock_events)
        self.mock_adapter.list_events.assert_called_once_with("primary", start_filter, end_filter, "task123")

    def test_block_time_for_task_via_schedule_agent(self):
        from task_agent.task_calendar_linker import block_time_for_task_via_schedule_agent
        mock_schedule = MagicMock()
        mock_schedule.process.return_value = {"status": "success", "data": {"event_id": "evt123"}}
        task_input = TaskInput(**VALID_TASK_DATA_DICT)
        event_id = block_time_for_task_via_schedule_agent(mock_schedule, "user1", task_input)
        self.assertEqual(event_id, "evt123")
        mock_schedule.process.assert_called_once()

    def test_block_time_for_task_method(self):
        self.linker._connected = True
        mock_schedule = MagicMock()
        mock_schedule.process.return_value = {"status": "success", "data": {"event_id": "evt321"}}
        self.mock_adapter.create_event.return_value = "evt321"

        task_input = TaskInput(**VALID_TASK_DATA_DICT)
        event_id = self.linker.block_time_for_task(mock_schedule, "user1", task_input)

        self.assertEqual(event_id, "evt321")
        mock_schedule.process.assert_called_once()
        self.mock_adapter.create_event.assert_called_once()

# Keep TestTaskInput and TestCalendarEvent as they are still relevant for the data models.
# Remove TestPlaceholderFunctions as those functions are gone.

if __name__ == '__main__':
    unittest.main()
