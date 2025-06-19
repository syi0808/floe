# tests/task_agent/test_task_calendar_linker.py
import pytest
import datetime
import logging
from unittest.mock import AsyncMock, MagicMock, patch # Added AsyncMock and MagicMock
from task_agent.task_calendar_linker import TaskCalendarLink, TaskCalendarLinker, MockScheduleAgentClient as ActualMockScheduleAgentClient, BaseScheduleAgentClient

# Keep original fixtures for existing tests that might rely on their simplicity
@pytest.fixture
def simple_linker_instance():
    # Uses the ActualMockScheduleAgentClient by default if no client is passed
    return TaskCalendarLinker()

@pytest.fixture
def simple_linker_with_mock_client(): # Renamed to avoid clash, uses the original simple mock
    class OriginalMockScheduleAgentClient(BaseScheduleAgentClient): # Copied from original tests
        async def create_event(self, *args, **kwargs):
            return {"id": f"evt_mock_client_{datetime.datetime.now(datetime.timezone.utc).timestamp()}", "status": "confirmed"}
        async def delete_event(self, event_id: str):
            logging.info(f"OriginalMockScheduleAgentClient: delete_event({event_id}) called") # Changed print to logging
            return {"status": "deleted", "deleted_event_id": event_id}
    return TaskCalendarLinker(schedule_agent_client=OriginalMockScheduleAgentClient())

# New fixture for more detailed mocking using unittest.mock.AsyncMock
@pytest.fixture
def linker_with_advanced_mock_client():
    mock_client = AsyncMock(spec=ActualMockScheduleAgentClient) # Use spec from the actual implementation's mock
    linker = TaskCalendarLinker(schedule_agent_client=mock_client)
    return linker, mock_client


# --- Original TaskCalendarLink Model Tests (Unchanged) ---
def test_task_calendar_link_creation_minimal():
    link = TaskCalendarLink(task_id="task123")
    assert link.task_id == "task123"
    assert link.status == "pending_creation"
    assert link.calendar_event_id is None

def test_task_calendar_link_creation_full():
    now = datetime.datetime.now(datetime.timezone.utc)
    link = TaskCalendarLink(
        task_id="task456",
        calendar_event_id="event789",
        calendar_service_provider="google_calendar",
        event_summary="Team Meeting",
        start_time=now,
        end_time=now + datetime.timedelta(hours=1),
        status="created",
        error_message=None,
        metadata={"project": "Floe"}
    )
    assert link.task_id == "task456"
    assert link.calendar_event_id == "event789"
    assert link.status == "created"
    assert link.metadata["project"] == "Floe"

# --- Original Tests for TaskCalendarLinker (using simple_linker_instance) ---
@pytest.mark.asyncio
async def test_create_calendar_event_insufficient_info(simple_linker_instance: TaskCalendarLinker):
    task_id = "task_no_time"
    description = "A task with no time"
    link = await simple_linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description
    )
    assert link.task_id == task_id
    assert link.status == "failed"
    assert link.calendar_event_id is None
    assert link.error_message is not None
    assert "Insufficient time information" in link.error_message

@pytest.mark.asyncio
async def test_create_calendar_event_with_start_and_duration(simple_linker_instance: TaskCalendarLinker):
    task_id = "task_start_duration"
    description = "Task with start and duration"
    start_time = datetime.datetime(2024, 7, 1, 10, 0, 0, tzinfo=datetime.timezone.utc)
    duration_minutes = 60
    link = await simple_linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_start_time=start_time,
        duration_minutes=duration_minutes
    )
    assert link.status == "created"
    assert link.calendar_event_id is not None
    assert link.start_time == start_time
    assert link.end_time == start_time + datetime.timedelta(minutes=duration_minutes)
    assert link.event_summary == f"Task: {description}"

@pytest.mark.asyncio
async def test_create_calendar_event_with_end_and_duration(simple_linker_instance: TaskCalendarLinker):
    task_id = "task_end_duration"
    description = "Task with end and duration"
    end_time = datetime.datetime(2024, 7, 1, 12, 0, 0, tzinfo=datetime.timezone.utc)
    duration_minutes = 90
    link = await simple_linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_end_time=end_time,
        duration_minutes=duration_minutes
    )
    assert link.status == "created"
    assert link.calendar_event_id is not None
    assert link.end_time == end_time
    assert link.start_time == end_time - datetime.timedelta(minutes=duration_minutes)

@pytest.mark.asyncio
async def test_create_calendar_event_with_only_duration_defaults_start_to_now(simple_linker_instance: TaskCalendarLinker):
    task_id = "task_only_duration"
    description = "Task with only duration"
    duration_minutes = 45
    before_call = datetime.datetime.now(datetime.timezone.utc)
    link = await simple_linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        duration_minutes=duration_minutes
    )
    after_call = datetime.datetime.now(datetime.timezone.utc)
    assert link.status == "created"
    assert link.calendar_event_id is not None
    assert link.start_time is not None
    link_start_time_utc = link.start_time.astimezone(datetime.timezone.utc) if link.start_time.tzinfo else link.start_time.replace(tzinfo=datetime.timezone.utc)
    assert before_call <= link_start_time_utc <= after_call

@pytest.mark.asyncio
async def test_create_calendar_event_with_original_mock_client(simple_linker_with_mock_client: TaskCalendarLinker): # Uses original simple mock
    task_id = "task_mock_client"
    description = "Task via mock client"
    start_time = datetime.datetime(2024, 7, 1, 14, 0, 0, tzinfo=datetime.timezone.utc)
    end_time = start_time + datetime.timedelta(hours=2)
    link = await simple_linker_with_mock_client.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_start_time=start_time,
        suggested_end_time=end_time
    )
    assert link.status == "created"
    assert "evt_mock_client" in link.calendar_event_id

@pytest.mark.asyncio
async def test_get_task_calendar_link_placeholder(simple_linker_instance: TaskCalendarLinker):
    result = await simple_linker_instance.get_task_calendar_link("some_task_id_not_created")
    assert result is None

@pytest.mark.asyncio
async def test_remove_calendar_event_for_task_placeholder(simple_linker_instance: TaskCalendarLinker):
    # This test relies on the default MockScheduleAgentClient in simple_linker_instance succeeding
    result = await simple_linker_instance.remove_calendar_event_for_task("task_id_to_remove", "event_id_to_remove")
    assert result is True

@pytest.mark.asyncio
async def test_remove_calendar_event_with_original_mock_client(simple_linker_with_mock_client: TaskCalendarLinker): # Uses original simple mock
    result = await simple_linker_with_mock_client.remove_calendar_event_for_task("task_abc", "event_xyz")
    assert result is True


# --- Enhanced Tests using linker_with_advanced_mock_client ---

@pytest.mark.asyncio
async def test_create_event_client_called_correctly(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_client_verify"
    description = "Verify client call"
    start_time = datetime.datetime(2024, 8, 1, 10, 0, 0, tzinfo=datetime.timezone.utc)
    duration_minutes = 60
    expected_end_time = start_time + datetime.timedelta(minutes=duration_minutes)
    mock_client.create_event.return_value = {"id": "evt_123_advanced", "status": "confirmed"}

    await linker.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_start_time=start_time,
        duration_minutes=duration_minutes
    )
    mock_client.create_event.assert_called_once_with(
        summary=f"Task: {description}",
        start_time=start_time,
        end_time=expected_end_time,
        description=f"Calendar event for task ID: {task_id}\n\n{description}"
    )

@pytest.mark.asyncio
async def test_create_event_client_returns_no_id(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    mock_client.create_event.return_value = {"status": "confirmed"} # No 'id'
    link = await linker.create_calendar_event_for_task(
        task_id="task_no_event_id",
        task_description="Client returns no event ID",
        suggested_start_time=datetime.datetime.now(datetime.timezone.utc),
        duration_minutes=30
    )
    assert link.status == "failed"
    assert "No event ID returned from client" in link.error_message

@pytest.mark.asyncio
async def test_create_event_client_raises_exception(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    mock_client.create_event.side_effect = Exception("Calendar API is down")
    link = await linker.create_calendar_event_for_task(
        task_id="task_client_exception",
        task_description="Client throws error",
        suggested_start_time=datetime.datetime.now(datetime.timezone.utc),
        duration_minutes=30
    )
    assert link.status == "failed"
    assert "Exception during calendar event creation: Calendar API is down" in link.error_message

@pytest.mark.asyncio
async def test_create_event_inconsistent_time_params(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_inconsistent_time"
    description = "Inconsistent time"
    start_time = datetime.datetime(2024, 8, 1, 10, 0, 0, tzinfo=datetime.timezone.utc)
    # end_time is 11:00, but duration is 90 minutes (so end_time should be 11:30)
    end_time = start_time + datetime.timedelta(hours=1)
    duration_minutes = 90

    expected_recalculated_end_time = start_time + datetime.timedelta(minutes=duration_minutes)
    mock_client.create_event.return_value = {"id": "evt_inconsistent", "status": "confirmed"}

    link = await linker.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_start_time=start_time,
        suggested_end_time=end_time,
        duration_minutes=duration_minutes
    )
    assert link.status == "created"
    assert link.start_time == start_time
    assert link.end_time == expected_recalculated_end_time # Verifies end_time was recalculated
    mock_client.create_event.assert_called_once_with(
        summary=f"Task: {description}",
        start_time=start_time,
        end_time=expected_recalculated_end_time,
        description=f"Calendar event for task ID: {task_id}\n\n{description}"
    )

@pytest.mark.asyncio
async def test_get_created_task_calendar_link(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_get_created"
    mock_client.create_event.return_value = {"id": "evt_get_this", "status": "confirmed"}

    created_link = await linker.create_calendar_event_for_task(
        task_id=task_id,
        task_description="Test get link",
        suggested_start_time=datetime.datetime.now(datetime.timezone.utc),
        duration_minutes=30
    )
    assert created_link.status == "created"

    retrieved_link = await linker.get_task_calendar_link(task_id)
    assert retrieved_link is not None
    assert retrieved_link.task_id == task_id
    assert retrieved_link.calendar_event_id == "evt_get_this"
    assert retrieved_link.status == "created"

@pytest.mark.asyncio
async def test_remove_event_client_called_correctly_and_status_updated(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_remove_ok"
    event_id = "evt_to_remove_123"

    # First, create a link to ensure it's in _links for status update check
    mock_client.create_event.return_value = {"id": event_id, "status": "confirmed"}
    await linker.create_calendar_event_for_task(
        task_id=task_id, task_description="Setup for remove",
        suggested_start_time=datetime.datetime.now(datetime.timezone.utc), duration_minutes=10
    )

    mock_client.delete_event.return_value = {"status": "deleted", "deleted_event_id": event_id}

    result = await linker.remove_calendar_event_for_task(task_id=task_id, event_id=event_id)
    assert result is True
    mock_client.delete_event.assert_called_once_with(event_id)

    updated_link = await linker.get_task_calendar_link(task_id)
    assert updated_link is not None
    assert updated_link.status == "deleted"

@pytest.mark.asyncio
async def test_remove_event_client_raises_exception(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_remove_fail_client_ex"
    event_id = "evt_fail_remove"

    mock_client.create_event.return_value = {"id": event_id, "status": "confirmed"}
    await linker.create_calendar_event_for_task(
        task_id=task_id, task_description="Setup for remove fail",
        suggested_start_time=datetime.datetime.now(datetime.timezone.utc), duration_minutes=10
    )

    mock_client.delete_event.side_effect = Exception("Calendar API error on delete")
    result = await linker.remove_calendar_event_for_task(task_id=task_id, event_id=event_id)
    assert result is False

    updated_link = await linker.get_task_calendar_link(task_id)
    assert updated_link is not None
    assert updated_link.status == "failed_deletion"
    assert "Calendar API error on delete" in updated_link.error_message

@pytest.mark.asyncio
async def test_remove_event_id_from_link_storage(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_remove_from_storage"
    event_id = "evt_in_storage_for_removal"

    mock_client.create_event.return_value = {"id": event_id, "status": "confirmed"}
    await linker.create_calendar_event_for_task(
        task_id=task_id, task_description="Event to be removed via stored ID",
        suggested_start_time=datetime.datetime.now(datetime.timezone.utc), duration_minutes=10
    )

    mock_client.delete_event.return_value = {"status": "deleted", "deleted_event_id": event_id}
    # Call remove_calendar_event_for_task WITHOUT event_id, expecting it to be fetched from _links
    result = await linker.remove_calendar_event_for_task(task_id=task_id)
    assert result is True
    mock_client.delete_event.assert_called_once_with(event_id)

    updated_link = await linker.get_task_calendar_link(task_id)
    assert updated_link.status == "deleted"

@pytest.mark.asyncio
async def test_remove_event_no_event_id_and_no_link(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_remove_no_info"

    # Ensure no link exists for this task_id
    assert await linker.get_task_calendar_link(task_id) is None

    result = await linker.remove_calendar_event_for_task(task_id=task_id) # No event_id provided
    assert result is False
    mock_client.delete_event.assert_not_called()

@pytest.mark.asyncio
async def test_create_event_with_naive_datetime_inputs(linker_with_advanced_mock_client):
    linker, mock_client = linker_with_advanced_mock_client
    task_id = "task_naive_dt"
    description = "Naive datetime input"
    # Naive datetime objects
    naive_start_time = datetime.datetime(2024, 8, 1, 10, 0, 0)
    duration_minutes = 60

    # Expected UTC versions
    expected_utc_start_time = naive_start_time.replace(tzinfo=datetime.timezone.utc)
    expected_utc_end_time = expected_utc_start_time + datetime.timedelta(minutes=duration_minutes)

    mock_client.create_event.return_value = {"id": "evt_naive_test", "status": "confirmed"}

    link = await linker.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_start_time=naive_start_time,
        duration_minutes=duration_minutes
    )

    assert link.status == "created"
    assert link.start_time == expected_utc_start_time
    assert link.end_time == expected_utc_end_time

    mock_client.create_event.assert_called_once_with(
        summary=f"Task: {description}",
        start_time=expected_utc_start_time,
        end_time=expected_utc_end_time,
        description=f"Calendar event for task ID: {task_id}\n\n{description}"
    )
