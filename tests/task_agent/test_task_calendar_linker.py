# tests/task_agent/test_task_calendar_linker.py
"""
Unit tests for the TaskCalendarLinker module.
"""

import unittest
from typing import Any, Dict, Optional

from task_agent.task_calendar_linker import (
    TaskCalendarLink,
    block_time_for_task,
    get_task_calendar_link,
    store_task_calendar_link
)


class TestTaskCalendarLinker(unittest.TestCase):
    """
    Test suite for TaskCalendarLinker functionalities.
    """

    def test_task_calendar_link_model(self):
        """
        Tests the creation and field assignment of the TaskCalendarLink model.
        """
        link_data = {
            "task_id": "task_123",
            "calendar_event_id": "event_abc",
            "calendar_service_id": "google_calendar",
            "details": {"notes": "Meeting about project X"}
        }
        link = TaskCalendarLink(**link_data)

        self.assertIsNotNone(link)
        self.assertEqual(link.task_id, "task_123")
        self.assertEqual(link.calendar_event_id, "event_abc")
        self.assertEqual(link.calendar_service_id, "google_calendar")
        self.assertEqual(link.details, {"notes": "Meeting about project X"})

        # Test with optional fields omitted
        link_minimal_data = {
            "task_id": "task_456",
            "calendar_event_id": "event_def",
        }
        link_minimal = TaskCalendarLink(**link_minimal_data)
        self.assertIsNotNone(link_minimal)
        self.assertEqual(link_minimal.task_id, "task_456")
        self.assertEqual(link_minimal.calendar_event_id, "event_def")
        self.assertIsNone(link_minimal.calendar_service_id)
        self.assertIsNone(link_minimal.details)

    def test_block_time_for_task_placeholder(self):
        """
        Tests the placeholder implementation of block_time_for_task.
        It should currently return None.
        """
        result = block_time_for_task(
            user_id="user_001",
            task_id="task_789",
            task_description="Plan next sprint",
            estimated_duration_hours=2,
            preferred_time_window={"preferred_time": "morning"}
        )
        self.assertIsNone(result, "block_time_for_task should return None as it's a placeholder.")

    def test_get_task_calendar_link_placeholder(self):
        """
        Tests the placeholder implementation of get_task_calendar_link.
        It should currently return None.
        """
        result = get_task_calendar_link(task_id="task_abc", user_id="user_002")
        self.assertIsNone(result, "get_task_calendar_link should return None as it's a placeholder.")

    def test_store_task_calendar_link_placeholder(self):
        """
        Tests the placeholder implementation of store_task_calendar_link.
        It should currently return False.
        """
        sample_link = TaskCalendarLink(
            task_id="task_xyz",
            calendar_event_id="event_123"
        )
        result = store_task_calendar_link(link=sample_link, user_id="user_003")
        self.assertFalse(result, "store_task_calendar_link should return False as it's a placeholder.")


if __name__ == '__main__':
    unittest.main()
