# tests/task_agent/test_task_calendar_linker.py

import pytest
from unittest.mock import patch, MagicMock
from uuid import uuid4, UUID
from datetime import datetime, timedelta, timezone

# Functions to test
from task_agent.task_calendar_linker import (
    block_time_for_task,
    get_linked_calendar_event_for_task,
    remove_calendar_block_for_task
)

@pytest.fixture
def sample_task_id() -> UUID:
    return uuid4()

@pytest.fixture
def sample_start_time() -> datetime:
    return datetime.now(timezone.utc) + timedelta(days=1)

@pytest.fixture
def sample_duration() -> timedelta:
    return timedelta(hours=1)

@pytest.fixture
def mock_calendar_service_api():
    return MagicMock()

# --- Tests for block_time_for_task ---

def test_block_time_for_task_with_service_success(
    sample_task_id, sample_start_time, sample_duration, mock_calendar_service_api
):
    """Test blocking time when a calendar service is provided (simulated success)."""
    with patch('builtins.print') as mock_print: # To capture print statements
        event_id = block_time_for_task(
            task_id=sample_task_id,
            start_time=sample_start_time,
            duration=sample_duration,
            calendar_service_api=mock_calendar_service_api
        )

    assert event_id is not None
    assert isinstance(event_id, str)
    assert f"cal_evt_{sample_task_id}" in event_id

    mock_print.assert_any_call(f"Placeholder: Blocking time for task {sample_task_id} from {sample_start_time} for {sample_duration}.")
    mock_print.assert_any_call(f"Placeholder: Interacting with calendar service: {mock_calendar_service_api}")
    mock_print.assert_any_call(f"Placeholder: Created calendar event {event_id}")

def test_block_time_for_task_no_service_failure(
    sample_task_id, sample_start_time, sample_duration
):
    """Test blocking time when no calendar service is provided (simulated failure)."""
    with patch('builtins.print') as mock_print:
        event_id = block_time_for_task(
            task_id=sample_task_id,
            start_time=sample_start_time,
            duration=sample_duration,
            calendar_service_api=None
        )

    assert event_id is None
    mock_print.assert_any_call(f"Placeholder: Blocking time for task {sample_task_id} from {sample_start_time} for {sample_duration}.")
    # Ensure no interaction or creation messages are printed
    interaction_call = f"Placeholder: Interacting with calendar service: {None}"
    creation_call_prefix = "Placeholder: Created calendar event"

    for call_args in mock_print.call_args_list:
        assert interaction_call not in call_args[0][0]
        assert creation_call_prefix not in call_args[0][0]


# --- Tests for get_linked_calendar_event_for_task ---

def test_get_linked_calendar_event_with_service_success(sample_task_id, mock_calendar_service_api):
    """Test getting linked event when service is provided (simulated success)."""
    with patch('builtins.print') as mock_print:
        event_details = get_linked_calendar_event_for_task(
            task_id=sample_task_id,
            calendar_service_api=mock_calendar_service_api
        )

    assert event_details is not None
    assert isinstance(event_details, dict)
    assert event_details["task_id"] == sample_task_id
    assert "event_id" in event_details
    assert f"cal_evt_{sample_task_id}" in event_details["event_id"]

    mock_print.assert_any_call(f"Placeholder: Getting linked calendar event for task {sample_task_id}.")
    mock_print.assert_any_call(f"Placeholder: Interacting with calendar service: {mock_calendar_service_api}")

def test_get_linked_calendar_event_no_service_failure(sample_task_id):
    """Test getting linked event when no service is provided (simulated failure)."""
    with patch('builtins.print') as mock_print:
        event_details = get_linked_calendar_event_for_task(
            task_id=sample_task_id,
            calendar_service_api=None
        )

    assert event_details is None
    mock_print.assert_any_call(f"Placeholder: Getting linked calendar event for task {sample_task_id}.")
    interaction_call = f"Placeholder: Interacting with calendar service: {None}"
    for call_args in mock_print.call_args_list:
        assert interaction_call not in call_args[0][0]

# --- Tests for remove_calendar_block_for_task ---

def test_remove_calendar_block_with_service_success(sample_task_id, mock_calendar_service_api):
    """Test removing calendar block when service is provided (simulated success)."""
    calendar_event_id_to_remove = f"cal_evt_{sample_task_id}_test_to_remove"
    with patch('builtins.print') as mock_print:
        result = remove_calendar_block_for_task(
            task_id=sample_task_id,
            calendar_event_id=calendar_event_id_to_remove,
            calendar_service_api=mock_calendar_service_api
        )

    assert result is True
    mock_print.assert_any_call(f"Placeholder: Removing calendar block {calendar_event_id_to_remove} for task {sample_task_id}.")
    mock_print.assert_any_call(f"Placeholder: Interacting with calendar service: {mock_calendar_service_api}")
    mock_print.assert_any_call(f"Placeholder: Deleted calendar event {calendar_event_id_to_remove}")

def test_remove_calendar_block_no_service_failure(sample_task_id):
    """Test removing calendar block when no service is provided (simulated failure)."""
    calendar_event_id_to_remove = f"cal_evt_{sample_task_id}_test_to_remove"
    with patch('builtins.print') as mock_print:
        result = remove_calendar_block_for_task(
            task_id=sample_task_id,
            calendar_event_id=calendar_event_id_to_remove,
            calendar_service_api=None
        )

    assert result is False
    mock_print.assert_any_call(f"Placeholder: Removing calendar block {calendar_event_id_to_remove} for task {sample_task_id}.")
    interaction_call = f"Placeholder: Interacting with calendar service: {None}"
    deletion_call = f"Placeholder: Deleted calendar event {calendar_event_id_to_remove}"
    for call_args in mock_print.call_args_list:
        assert interaction_call not in call_args[0][0]
        assert deletion_call not in call_args[0][0]
