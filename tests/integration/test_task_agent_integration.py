import pytest
from datetime import datetime, timedelta, timezone
import uuid

# Modules to be tested or used
from task_agent import task_core
from task_agent.task_calendar_linker import TaskCalendarLinker, TaskInput, CalendarEvent
from tests.integration.mocks.mock_calendar_adapter import MockCalendarAdapter
from tests.integration.mocks.mock_memory_manager_agent import MockMemoryManagerAgent

USER_ID = "test_user_123"

# --- Fixtures ---

@pytest.fixture(autouse=True)
def clear_task_storage():
    """Ensures _task_storage in task_core is empty before each test."""
    task_core._task_storage.clear()
    yield
    task_core._task_storage.clear()

@pytest.fixture
def mock_calendar_adapter():
    adapter = MockCalendarAdapter()
    adapter.connect() # Connect the mock adapter
    return adapter

@pytest.fixture
def task_calendar_linker(mock_calendar_adapter):
    linker = TaskCalendarLinker(adapter=mock_calendar_adapter)
    # TaskCalendarLinker needs to be connected as well
    # In its own connect_calendar method, it calls adapter.connect()
    # but since we are passing an already connected adapter, this is fine.
    # For safety, we can call its connect method.
    linker.connect_calendar()
    return linker

@pytest.fixture
def mock_memory_manager_agent():
    return MockMemoryManagerAgent()

# --- Test Scenarios ---

def test_scenario_1_create_task_with_calendar_event(
    task_calendar_linker: TaskCalendarLinker,
    mock_calendar_adapter: MockCalendarAdapter,
    mock_memory_manager_agent: MockMemoryManagerAgent
):
    """
    Scenario 1: Create Task with Calendar Event
    - Input: Task details (description, due date, priority).
    - Action:
        1. Create a task using task_core.create_task.
        2. Create a calendar event using TaskCalendarLinker.
        3. Link the event to the task by updating the task's linked_schedule_id.
        4. (Optional) Store task in memory manager.
    - Expected Outcome:
        - Task is created in TaskAgent's internal storage (_task_storage).
        - A corresponding event is created in the calendar (mock_calendar_adapter.events).
        - The link between the task and calendar event is established (task.linked_schedule_id).
        - Task data is stored in memory manager (mock_memory_manager_agent).
    """
    # 1. Create a task
    due_date = datetime.now(timezone.utc) + timedelta(days=5)
    task_description = "Integration Test Task 1"
    created_task = task_core.create_task(
        user_id=USER_ID,
        description=task_description,
        due_date_utc=due_date,
        priority=1
    )
    assert created_task is not None
    assert str(created_task.id) in task_core._task_storage

    # 2. Create a calendar event for this task
    task_input_for_calendar = TaskInput(
        task_id=str(created_task.id),
        description=created_task.description,
        start_time=due_date - timedelta(hours=1), # Assume event starts 1hr before due
        duration_minutes=60,
        summary=f"Task: {created_task.description}"
    )

    floe_event_id = task_calendar_linker.add_task_to_calendar(task_input_for_calendar, calendar_target="primary")
    assert floe_event_id is not None
    assert floe_event_id in mock_calendar_adapter.events

    calendar_event_in_mock = mock_calendar_adapter.events[floe_event_id]
    assert calendar_event_in_mock.summary == f"Task: {created_task.description}"
    assert calendar_event_in_mock.task_id_ref == str(created_task.id)

    # 3. Link the event to the task
    updated_task = task_core.update_task(str(created_task.id), {"linked_schedule_id": floe_event_id})
    assert updated_task is not None
    assert updated_task.linked_schedule_id == floe_event_id
    assert task_core._task_storage[str(created_task.id)].linked_schedule_id == floe_event_id

    # 4. (Optional) Store task in memory manager
    memory_item = {
        "type": "task_data",
        "content": updated_task.model_dump_json(), # Store as JSON string
        "task_id": str(updated_task.id)
    }
    mock_memory_manager_agent.add_memory(user_id=USER_ID, memory_item=memory_item)

    user_memories = mock_memory_manager_agent.get_user_memories(USER_ID)
    assert len(user_memories) == 1
    assert user_memories[0]["task_id"] == str(updated_task.id)

    print(f"Scenario 1 PASSED: Task {created_task.id} created, linked to event {floe_event_id}, and stored in memory.")


def test_scenario_2_update_task_and_calendar_event(
    task_calendar_linker: TaskCalendarLinker,
    mock_calendar_adapter: MockCalendarAdapter,
    mock_memory_manager_agent: MockMemoryManagerAgent
):
    """
    Scenario 2: Update Task and its Calendar Event
    - Prerequisite: A task with a linked calendar event exists.
    - Input: Updated task details (e.g., new due date, new description).
    - Action:
        1. Update task details in task_core.
        2. Update the corresponding calendar event using TaskCalendarLinker.
        3. (Optional) Update task in memory manager.
    - Expected Outcome:
        - Task details are updated in task_core.
        - The corresponding calendar event is updated in mock_calendar_adapter.
        - Task data is updated in mock_memory_manager_agent.
    """
    # Prerequisite: Create a task and a linked calendar event (similar to scenario 1)
    initial_due_date = datetime.now(timezone.utc) + timedelta(days=3)
    initial_description = "Initial Task for Update"
    task_to_update = task_core.create_task(
        user_id=USER_ID,
        description=initial_description,
        due_date_utc=initial_due_date,
        priority=2
    )
    task_id_str = str(task_to_update.id)

    task_input_for_calendar_initial = TaskInput(
        task_id=task_id_str,
        description=initial_description,
        start_time=initial_due_date - timedelta(hours=1),
        duration_minutes=45,
        summary=f"Task: {initial_description}"
    )
    initial_floe_event_id = task_calendar_linker.add_task_to_calendar(task_input_for_calendar_initial, "primary")
    assert initial_floe_event_id is not None
    task_core.update_task(task_id_str, {"linked_schedule_id": initial_floe_event_id})

    # Store initial version in memory
    mock_memory_manager_agent.add_memory(USER_ID, {"type": "task_data", "content": task_to_update.model_dump_json(), "task_id": task_id_str})


    # 1. Update task details
    new_description = "Updated Task Description for Scenario 2"
    new_due_date = datetime.now(timezone.utc) + timedelta(days=7)
    updates_for_core = {
        "description": new_description,
        "due_date_utc": new_due_date,
        "priority": 1
    }
    updated_task_from_core = task_core.update_task(task_id_str, updates_for_core)
    assert updated_task_from_core is not None
    assert updated_task_from_core.description == new_description
    assert updated_task_from_core.due_date_utc == new_due_date
    assert updated_task_from_core.priority == 1

    # 2. Update the corresponding calendar event
    task_input_for_calendar_update = TaskInput(
        task_id=task_id_str, # task_id remains the same
        description=new_description,
        start_time=new_due_date - timedelta(hours=1), # New start time based on new due date
        duration_minutes=75, # New duration
        summary=f"Task: {new_description}" # New summary
    )

    # The floe_event_id for the event to be updated is initial_floe_event_id
    update_success = task_calendar_linker.update_linked_event(
        floe_event_id=initial_floe_event_id,
        task_input=task_input_for_calendar_update,
        calendar_target="primary"
    )
    assert update_success is True

    updated_event_in_mock = mock_calendar_adapter.events.get(initial_floe_event_id)
    assert updated_event_in_mock is not None
    assert updated_event_in_mock.summary == f"Task: {new_description}"
    assert updated_event_in_mock.end_time == (new_due_date - timedelta(hours=1) + timedelta(minutes=75))

    # 3. (Optional) Update task in memory manager
    # For simplicity, let's assume we remove the old one and add the new one,
    # or MemoryManagerAgent would have an update_memory method.
    mock_memory_manager_agent.clear_memory(USER_ID) # Clear old
    mock_memory_manager_agent.add_memory(USER_ID, {"type": "task_data", "content": updated_task_from_core.model_dump_json(), "task_id": task_id_str})

    user_memories = mock_memory_manager_agent.get_user_memories(USER_ID)
    assert len(user_memories) == 1
    # Basic check, could parse JSON and check specific fields
    assert new_description in user_memories[0]["content"]

    print(f"Scenario 2 PASSED: Task {task_id_str} and its event {initial_floe_event_id} updated.")

def test_scenario_3_delete_task_and_calendar_event(
    task_calendar_linker: TaskCalendarLinker,
    mock_calendar_adapter: MockCalendarAdapter,
    mock_memory_manager_agent: MockMemoryManagerAgent
):
    """
    Scenario 3: Delete Task and its Calendar Event
    - Prerequisite: A task with a linked calendar event exists.
    - Action:
        1. Delete the task from task_core.
        2. Delete the calendar event using TaskCalendarLinker.
        3. (Optional) Remove task data from MemoryManagerAgent.
    - Expected Outcome:
        - Task is removed from task_core._task_storage.
        - The corresponding calendar event is removed from mock_calendar_adapter.events.
        - Task data is removed from mock_memory_manager_agent.
    """
    # Prerequisite: Create a task and a linked calendar event
    due_date = datetime.now(timezone.utc) + timedelta(days=1)
    task_description = "Task for Deletion"
    task_to_delete = task_core.create_task(
        user_id=USER_ID,
        description=task_description,
        due_date_utc=due_date
    )
    task_id_str = str(task_to_delete.id)

    task_input_for_calendar = TaskInput(
        task_id=task_id_str,
        description=task_description,
        start_time=due_date - timedelta(hours=1),
        duration_minutes=60,
        summary=f"Task: {task_description}"
    )
    floe_event_id = task_calendar_linker.add_task_to_calendar(task_input_for_calendar, "primary")
    assert floe_event_id is not None
    task_core.update_task(task_id_str, {"linked_schedule_id": floe_event_id})

    # Store in memory
    mock_memory_manager_agent.add_memory(USER_ID, {"type": "task_data", "content": task_to_delete.model_dump_json(), "task_id": task_id_str})
    assert len(mock_memory_manager_agent.get_user_memories(USER_ID)) == 1
    assert floe_event_id in mock_calendar_adapter.events
    assert task_id_str in task_core._task_storage

    # 1. Delete the task from task_core
    delete_core_success = task_core.delete_task(task_id_str)
    assert delete_core_success is True
    assert task_id_str not in task_core._task_storage

    # 2. Delete the calendar event using TaskCalendarLinker
    # We need the floe_event_id which was stored in task_to_delete.linked_schedule_id
    # Since task_to_delete is now removed from core, we use the variable `floe_event_id` captured earlier.
    delete_calendar_success = task_calendar_linker.remove_task_from_calendar(floe_event_id, "primary")
    assert delete_calendar_success is True
    assert floe_event_id not in mock_calendar_adapter.events

    # 3. (Optional) Remove task data from MemoryManagerAgent
    # Assuming a method like remove_memory_by_task_id or clear specific memory.
    # For this mock, we can filter or clear and re-evaluate.
    # Let's simulate removal by task_id if such a method existed, or just clear for test.
    all_memories = mock_memory_manager_agent.get_user_memories(USER_ID)
    updated_memories = [mem for mem in all_memories if mem.get("task_id") != task_id_str]

    mock_memory_manager_agent.clear_memory(USER_ID)
    for mem in updated_memories:
        mock_memory_manager_agent.add_memory(USER_ID, mem)

    assert len(mock_memory_manager_agent.get_user_memories(USER_ID)) == 0

    print(f"Scenario 3 PASSED: Task {task_id_str} and its event {floe_event_id} deleted.")
