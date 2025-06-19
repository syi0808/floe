# tests/task_agent/test_task_calendar_linker.py
import pytest
import datetime
from task_agent.task_calendar_linker import TaskCalendarLink, TaskCalendarLinker

@pytest.fixture
def linker_instance():
    return TaskCalendarLinker()

@pytest.fixture
def linker_with_mock_client():
    class MockScheduleAgentClient:
        async def create_event(self, *args, **kwargs):
            # In a real scenario, this mock would be more sophisticated,
            # potentially checking args or using a library like unittest.mock.
            # For now, it just returns a structure that the linker might expect.
            return {"id": f"evt_mock_client_{datetime.datetime.now(datetime.timezone.utc).timestamp()}", "status": "confirmed"}

        async def delete_event(self, event_id: str):
            # Similarly, a simple mock for deletion.
            print(f"MockScheduleAgentClient: delete_event({event_id}) called")
            return {"status": "deleted", "deleted_event_id": event_id}

    return TaskCalendarLinker(schedule_agent_client=MockScheduleAgentClient())

# Tests for TaskCalendarLink model
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

# Tests for TaskCalendarLinker class
@pytest.mark.asyncio
async def test_create_calendar_event_insufficient_info(linker_instance: TaskCalendarLinker):
    task_id = "task_no_time"
    description = "A task with no time"
    # Note: The implementation defaults to utcnow if only duration is given.
    # To truly test "insufficient info", we must provide nothing for time.
    link = await linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description
        # No suggested_start_time, suggested_end_time, or duration_minutes
    )
    assert link.task_id == task_id
    assert link.status == "failed"
    assert link.calendar_event_id is None
    assert link.error_message is not None
    assert "Insufficient time information" in link.error_message

@pytest.mark.asyncio
async def test_create_calendar_event_with_start_and_duration(linker_instance: TaskCalendarLinker):
    task_id = "task_start_duration"
    description = "Task with start and duration"
    # Ensure timezone-aware datetime for consistency, as utcnow() is timezone-aware (UTC)
    start_time = datetime.datetime(2024, 7, 1, 10, 0, 0, tzinfo=datetime.timezone.utc)
    duration_minutes = 60

    link = await linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_start_time=start_time,
        duration_minutes=duration_minutes
    )
    assert link.task_id == task_id
    assert link.status == "created"
    assert link.calendar_event_id is not None
    assert link.start_time == start_time
    assert link.end_time == start_time + datetime.timedelta(minutes=duration_minutes)
    assert link.event_summary == f"Task: {description}"

@pytest.mark.asyncio
async def test_create_calendar_event_with_end_and_duration(linker_instance: TaskCalendarLinker):
    task_id = "task_end_duration"
    description = "Task with end and duration"
    end_time = datetime.datetime(2024, 7, 1, 12, 0, 0, tzinfo=datetime.timezone.utc)
    duration_minutes = 90

    link = await linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_end_time=end_time,
        duration_minutes=duration_minutes
    )
    assert link.task_id == task_id
    assert link.status == "created"
    assert link.calendar_event_id is not None
    assert link.end_time == end_time
    assert link.start_time == end_time - datetime.timedelta(minutes=duration_minutes)
    assert link.event_summary == f"Task: {description}"

@pytest.mark.asyncio
async def test_create_calendar_event_with_only_duration_defaults_start_to_now(linker_instance: TaskCalendarLinker):
    task_id = "task_only_duration"
    description = "Task with only duration"
    duration_minutes = 45

    # Capture time just before the call
    # Ensure tz_aware for comparison, matching Pydantic model behavior / now(timezone.utc)
    before_call = datetime.datetime.now(datetime.timezone.utc)

    link = await linker_instance.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        duration_minutes=duration_minutes
    )

    # Capture time just after the call
    after_call = datetime.datetime.now(datetime.timezone.utc)

    assert link.task_id == task_id
    assert link.status == "created"
    assert link.calendar_event_id is not None
    assert link.start_time is not None

    # Ensure start_time from link is tz-aware if it's not already for comparison
    link_start_time_utc = link.start_time
    if link_start_time_utc.tzinfo is None or link_start_time_utc.tzinfo.utcoffset(link_start_time_utc) is None:
        link_start_time_utc = link_start_time_utc.replace(tzinfo=datetime.timezone.utc)

    link_end_time_utc = link.end_time
    if link_end_time_utc.tzinfo is None or link_end_time_utc.tzinfo.utcoffset(link_end_time_utc) is None:
        link_end_time_utc = link_end_time_utc.replace(tzinfo=datetime.timezone.utc)

    assert before_call <= link_start_time_utc <= after_call
    assert link_end_time_utc == link_start_time_utc + datetime.timedelta(minutes=duration_minutes)
    assert link.event_summary == f"Task: {description}"


@pytest.mark.asyncio
async def test_create_calendar_event_with_mock_client(linker_with_mock_client: TaskCalendarLinker):
    task_id = "task_mock_client"
    description = "Task via mock client"
    start_time = datetime.datetime(2024, 7, 1, 14, 0, 0, tzinfo=datetime.timezone.utc)
    end_time = start_time + datetime.timedelta(hours=2)

    link = await linker_with_mock_client.create_calendar_event_for_task(
        task_id=task_id,
        task_description=description,
        suggested_start_time=start_time,
        suggested_end_time=end_time
    )
    assert link.task_id == task_id
    assert link.status == "created"
    assert link.calendar_event_id is not None
    assert "evt_mock_client" in link.calendar_event_id # Check if it's from the mock
    assert link.start_time == start_time
    assert link.end_time == end_time

@pytest.mark.asyncio
async def test_get_task_calendar_link_placeholder(linker_instance: TaskCalendarLinker):
    result = await linker_instance.get_task_calendar_link("some_task_id")
    assert result is None

@pytest.mark.asyncio
async def test_remove_calendar_event_for_task_placeholder(linker_instance: TaskCalendarLinker):
    result = await linker_instance.remove_calendar_event_for_task("task_id_to_remove", "event_id_to_remove")
    assert result is True

@pytest.mark.asyncio
async def test_remove_calendar_event_with_mock_client(linker_with_mock_client: TaskCalendarLinker):
    result = await linker_with_mock_client.remove_calendar_event_for_task("task_abc", "event_xyz")
    assert result is True # Mock client simulates success
    # In a more complex mock, you might check that schedule_agent_client.delete_event was called.
    # For example, by adding a flag or using unittest.mock.AsyncMock features.
