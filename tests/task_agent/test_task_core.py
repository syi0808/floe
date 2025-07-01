import pytest
from uuid import UUID, uuid4
from datetime import datetime, timedelta, timezone

# Assuming Pydantic v2, ValidationError is the correct exception
from pydantic import ValidationError

# Module to be tested
from task_agent import task_core

# --- Test Fixtures & Setup ---


@pytest.fixture(autouse=True)
def clear_storage_before_each_test():
    """Ensures _task_storage is empty before each test."""
    task_core._task_storage.clear()
    yield  # Test runs here
    task_core._task_storage.clear()  # Optional: clear after test too


# --- TaskItem Model Tests ---


def test_taskitem_defaults():
    """Test TaskItem default values."""
    task = task_core.TaskItem(user_id="user1", description="Test task")
    assert isinstance(task.id, UUID)
    assert isinstance(task.created_at, datetime)
    assert task.created_at.tzinfo == timezone.utc
    assert task.priority == 2
    assert task.status == "todo"
    assert task.due_date_utc is None
    assert task.completed_at is None
    assert task.project_tags is None
    assert task.linked_schedule_id is None


def test_taskitem_priority_validation():
    """Test TaskItem priority validation (1-4)."""
    with pytest.raises(ValidationError):
        task_core.TaskItem(user_id="user1", description="Prio 0", priority=0)
    with pytest.raises(ValidationError):
        task_core.TaskItem(user_id="user1", description="Prio 5", priority=5)

    task_prio1 = task_core.TaskItem(user_id="user1", description="Prio 1", priority=1)
    assert task_prio1.priority == 1
    task_prio4 = task_core.TaskItem(user_id="user1", description="Prio 4", priority=4)
    assert task_prio4.priority == 4


def test_taskitem_status_validation():
    """Test TaskItem status validation (Literal values)."""
    with pytest.raises(ValidationError):
        task_core.TaskItem(
            user_id="user1", description="Invalid status", status="pending"
        )

    task_valid_status = task_core.TaskItem(
        user_id="user1", description="Valid status", status="in-progress"
    )
    assert task_valid_status.status == "in-progress"


def test_taskitem_serialization():
    """Test TaskItem JSON serialization for UUID and datetime."""
    now = datetime.now(timezone.utc)
    task = task_core.TaskItem(
        user_id="user_ser",
        description="Serialization test",
        created_at=now,  # Explicitly set to check format
        due_date_utc=now + timedelta(days=1),
    )
    task_dict = task.model_dump()  # Pydantic v2

    assert isinstance(
        task_dict["id"], UUID
    )  # model_dump keeps original types if not specified by json_encoders target
    assert task_dict["created_at"] == now
    assert task_dict["due_date_utc"] == now + timedelta(days=1)

    # Test json_encoders effect (usually applied during model_dump_json or when FastAPI returns the model)
    task_json_dict = task.model_dump(
        mode="json"
    )  # Pydantic v2, mode='json' applies json_encoders

    assert isinstance(task_json_dict["id"], str)
    assert task_json_dict["created_at"] == now.isoformat().replace(
        "+00:00", "Z"
    )  # Pydantic v2 default for datetimes
    assert task_json_dict["due_date_utc"] == (
        now + timedelta(days=1)
    ).isoformat().replace("+00:00", "Z")


# --- CRUD Function Tests ---


# create_task Tests
def test_create_task_basic():
    """Test basic task creation."""
    task = task_core.create_task("user1", "New Task 1")
    assert task.user_id == "user1"
    assert task.description == "New Task 1"
    assert task.priority == 2  # Default
    assert task.status == "todo"  # Default
    assert str(task.id) in task_core._task_storage
    assert task_core._task_storage[str(task.id)] == task


def test_create_task_with_all_fields():
    """Test task creation with all fields specified."""
    due = datetime.now(timezone.utc) + timedelta(days=5)
    task = task_core.create_task(
        user_id="user2",
        description="Detailed Task",
        due_date_utc=due,
        priority=1,
        project_tags=["projectX", "urgent"],
        status="in-progress",
    )
    assert task.user_id == "user2"
    assert task.description == "Detailed Task"
    assert task.due_date_utc == due
    assert task.priority == 1
    assert task.project_tags == ["projectX", "urgent"]
    assert task.status == "in-progress"
    assert str(task.id) in task_core._task_storage


def test_priority_auto_calculation():
    """Priority should be computed from due date when not provided."""
    soon = datetime.now(timezone.utc) + timedelta(hours=12)
    task = task_core.create_task("user3", "Soon task", due_date_utc=soon)
    assert task.priority == 1  # due within 1 day -> highest priority


# get_task Tests
def test_get_task_existing():
    """Test retrieving an existing task."""
    task = task_core.create_task("user1", "Task to get")
    retrieved_task = task_core.get_task(str(task.id))
    assert retrieved_task == task


def test_get_task_non_existent():
    """Test retrieving a non-existent task."""
    with pytest.raises(ValueError):
        task_core.get_task(str(uuid4()))


# update_task Tests
def test_update_task_existing():
    """Test updating an existing task."""
    task = task_core.create_task("user1", "Task to update")
    original_id = task.id
    original_created_at = task.created_at

    updates = {
        "description": "Updated description",
        "priority": 3,
        "status": "done",
        "project_tags": ["updated"],
    }
    updated_task = task_core.update_task(str(task.id), None, updates)

    assert updated_task is not None
    assert updated_task.id == original_id  # ID should not change
    assert (
        updated_task.created_at == original_created_at
    )  # created_at should not change
    assert updated_task.description == "Updated description"
    assert updated_task.priority == 3
    assert updated_task.status == "done"
    assert updated_task.project_tags == ["updated"]
    assert task_core._task_storage[str(task.id)] == updated_task


def test_update_task_partial():
    """Test partially updating an existing task."""
    task = task_core.create_task("user1", "Partial update task", priority=1)
    updates = {"status": "in-progress"}
    updated_task = task_core.update_task(str(task.id), None, updates)

    assert updated_task is not None
    assert updated_task.status == "in-progress"
    assert updated_task.description == "Partial update task"  # Unchanged
    assert updated_task.priority == 1  # Unchanged


def test_update_task_due_date_recalculates_priority():
    task = task_core.create_task(
        "user1",
        "Update me",
        due_date_utc=datetime.now(timezone.utc) + timedelta(days=10),
    )
    new_due = datetime.now(timezone.utc) + timedelta(hours=12)
    updates = {"due_date_utc": new_due}
    updated_task = task_core.update_task(str(task.id), None, updates)
    assert updated_task.priority == 1


def test_update_task_non_existent():
    """Test updating a non-existent task."""
    updates = {"description": "Doesn't matter"}
    with pytest.raises(ValueError):
        task_core.update_task(str(uuid4()), None, updates)


def test_update_task_invalid_data():
    """Test updating with invalid data (e.g., wrong priority value)."""
    task = task_core.create_task("user1", "Task for invalid update")
    updates_invalid_priority = {"priority": 5}  # Invalid priority
    with pytest.raises(ValueError):
        task_core.update_task(str(task.id), None, updates_invalid_priority)

    # Check that the original task was not modified
    original_task = task_core.get_task(str(task.id))
    assert original_task.priority == 2  # Default priority

    updates_invalid_status = {"status": "invalid_status_value"}
    with pytest.raises(ValueError):
        task_core.update_task(str(task.id), None, updates_invalid_status)
    original_task_after_failed_status_update = task_core.get_task(str(task.id))
    assert original_task_after_failed_status_update.status == "todo"


# delete_task Tests
def test_delete_task_existing():
    """Test deleting an existing task."""
    task = task_core.create_task("user1", "Task to delete")
    task_id_str = str(task.id)
    assert task_id_str in task_core._task_storage

    delete_result = task_core.delete_task(task_id_str)
    assert delete_result is True
    assert task_id_str not in task_core._task_storage
    with pytest.raises(ValueError):
        task_core.get_task(task_id_str)


def test_delete_task_non_existent():
    """Test deleting a non-existent task."""
    with pytest.raises(ValueError):
        task_core.delete_task(str(uuid4()))


# list_tasks Tests
@pytest.fixture
def sample_tasks():
    """Create a set of sample tasks for list_tasks tests."""
    now = datetime.now(timezone.utc)
    # User 1 tasks
    task1_u1 = task_core.create_task(
        "user1",
        "U1 Task 1 (Todo, Prio 2)",
        priority=2,
        status="todo",
        project_tags=["t1", "common"],
    )
    task2_u1 = task_core.create_task(
        "user1",
        "U1 Task 2 (In-Prog, Prio 1)",
        priority=1,
        status="in-progress",
        due_date_utc=now + timedelta(days=2),
        project_tags=["t2", "common"],
    )
    task3_u1 = task_core.create_task(
        "user1",
        "U1 Task 3 (Done, Prio 3)",
        priority=3,
        status="done",
        due_date_utc=now - timedelta(days=1),
    )

    # User 2 tasks
    task1_u2 = task_core.create_task(
        "user2",
        "U2 Task 1 (Todo, Prio 2)",
        priority=2,
        status="todo",
        project_tags=["t1"],
    )

    # To test sorting by created_at for same priority
    # Ensure task5_u1 is created slightly after task1_u1 if they have same priority
    # Forcing created_at for precise sorting tests (not ideal, but works for this)
    task1_u1.created_at = now - timedelta(seconds=10)
    task_core._task_storage[str(task1_u1.id)] = (
        task1_u1  # Re-store if create_task doesn't allow created_at override
    )

    task5_u1 = task_core.TaskItem(
        user_id="user1",
        description="U1 Task 5 (Todo, Prio 2, Newer)",
        priority=2,
        status="todo",
        created_at=now,
    )
    task_core._task_storage[str(task5_u1.id)] = task5_u1

    return {
        "user1": [task1_u1, task2_u1, task3_u1, task5_u1],  # task5_u1 added
        "user2": [task1_u2],
    }


def test_list_tasks_by_user_id(sample_tasks):
    """Test listing tasks only for a specific user."""
    user1_tasks_retrieved = task_core.list_tasks(user_id="user1")
    # Expected order: Prio 1 (task2_u1), Prio 2 (task1_u1, older), Prio 2 (task5_u1, newer), Prio 3 (task3_u1)
    assert len(user1_tasks_retrieved) == 4
    assert (
        user1_tasks_retrieved[0].description == "U1 Task 2 (In-Prog, Prio 1)"
    )  # Prio 1
    assert (
        user1_tasks_retrieved[1].description == "U1 Task 1 (Todo, Prio 2)"
    )  # Prio 2, older
    assert (
        user1_tasks_retrieved[2].description == "U1 Task 5 (Todo, Prio 2, Newer)"
    )  # Prio 2, newer
    assert user1_tasks_retrieved[3].description == "U1 Task 3 (Done, Prio 3)"  # Prio 3

    user2_tasks_retrieved = task_core.list_tasks(user_id="user2")
    assert len(user2_tasks_retrieved) == 1
    assert user2_tasks_retrieved[0].description == "U2 Task 1 (Todo, Prio 2)"

    no_user_tasks = task_core.list_tasks(user_id="non_existent_user")
    assert len(no_user_tasks) == 0


def test_list_tasks_by_status(sample_tasks):
    """Test filtering tasks by status."""
    inprogress_tasks = task_core.list_tasks(user_id="user1", status="in-progress")
    assert len(inprogress_tasks) == 1
    assert inprogress_tasks[0].description == "U1 Task 2 (In-Prog, Prio 1)"

    todo_tasks = task_core.list_tasks(user_id="user1", status="todo")
    assert len(todo_tasks) == 2  # task1_u1 and task5_u1
    assert todo_tasks[0].description == "U1 Task 1 (Todo, Prio 2)"
    assert todo_tasks[1].description == "U1 Task 5 (Todo, Prio 2, Newer)"


def test_list_tasks_by_project_tag(sample_tasks):
    """Test filtering tasks by project tag."""
    common_tag_tasks = task_core.list_tasks(user_id="user1", project_tag="common")
    assert len(common_tag_tasks) == 2
    descriptions = {t.description for t in common_tag_tasks}
    assert "U1 Task 2 (In-Prog, Prio 1)" in descriptions  # Prio 1
    assert "U1 Task 1 (Todo, Prio 2)" in descriptions  # Prio 2

    t1_tag_tasks = task_core.list_tasks(user_id="user1", project_tag="t1")
    assert len(t1_tag_tasks) == 1
    assert t1_tag_tasks[0].description == "U1 Task 1 (Todo, Prio 2)"

    non_existent_tag_tasks = task_core.list_tasks(
        user_id="user1", project_tag="non_existent_tag"
    )
    assert len(non_existent_tag_tasks) == 0


def test_list_tasks_by_due_date(sample_tasks):
    """Test filtering tasks by due date range."""
    now = datetime.now(timezone.utc)

    # Task 2 for user1 is due in 2 days
    due_soon_tasks = task_core.list_tasks(
        user_id="user1",
        due_date_start=now + timedelta(days=1),
        due_date_end=now + timedelta(days=3),
    )
    assert len(due_soon_tasks) == 1
    assert due_soon_tasks[0].description == "U1 Task 2 (In-Prog, Prio 1)"

    # Task 3 for user1 was due yesterday
    past_due_tasks = task_core.list_tasks(
        user_id="user1", due_date_end=now - timedelta(hours=1)  # Any time before now
    )
    assert len(past_due_tasks) == 1
    assert past_due_tasks[0].description == "U1 Task 3 (Done, Prio 3)"

    # No tasks due in far future
    far_future_tasks = task_core.list_tasks(
        user_id="user1", due_date_start=now + timedelta(days=10)
    )
    assert len(far_future_tasks) == 0


def test_list_tasks_combined_filters(sample_tasks):
    """Test combined filters for list_tasks."""
    # User1, status 'todo', project_tag 't1'
    filtered_tasks = task_core.list_tasks(
        user_id="user1", status="todo", project_tag="t1"
    )
    assert len(filtered_tasks) == 1
    assert filtered_tasks[0].description == "U1 Task 1 (Todo, Prio 2)"


def test_list_tasks_sorting(sample_tasks):
    """Test sorting order of list_tasks (priority then created_at)."""
    # This is implicitly tested in test_list_tasks_by_user_id, but can be more specific
    # For user1:
    # Task2 Prio 1
    # Task1 Prio 2 (older)
    # Task5 Prio 2 (newer)
    # Task3 Prio 3

    # Create tasks with same priority but different creation times for user 'sort_test_user'
    # Manually set created_at for predictable sorting
    # Note: Directly modifying _task_storage and created_at is for test setup convenience.
    # In a real scenario, created_at is set at instantiation.

    time_now = datetime.now(timezone.utc)

    st1 = task_core.TaskItem(
        user_id="sort_test_user",
        description="ST1 Prio2 Second",
        priority=2,
        created_at=time_now,
    )
    task_core._task_storage[str(st1.id)] = st1

    st2 = task_core.TaskItem(
        user_id="sort_test_user",
        description="ST2 Prio1 First",
        priority=1,
        created_at=time_now + timedelta(seconds=1),
    )  # Prio 1, should be first
    task_core._task_storage[str(st2.id)] = st2

    st3 = task_core.TaskItem(
        user_id="sort_test_user",
        description="ST3 Prio2 First",
        priority=2,
        created_at=time_now - timedelta(seconds=1),
    )  # Prio 2, older, should be after Prio 1
    task_core._task_storage[str(st3.id)] = st3

    st4 = task_core.TaskItem(
        user_id="sort_test_user",
        description="ST4 Prio3 Last",
        priority=3,
        created_at=time_now + timedelta(seconds=2),
    )
    task_core._task_storage[str(st4.id)] = st4

    sorted_tasks = task_core.list_tasks("sort_test_user")

    assert len(sorted_tasks) == 4
    assert sorted_tasks[0].description == "ST2 Prio1 First"
    assert sorted_tasks[1].description == "ST3 Prio2 First"  # Prio 2, older created_at
    assert sorted_tasks[2].description == "ST1 Prio2 Second"  # Prio 2, newer created_at
    assert sorted_tasks[3].description == "ST4 Prio3 Last"


def test_list_tasks_empty_result():
    """Test list_tasks when no tasks match criteria or user has no tasks."""
    no_tasks_user = task_core.list_tasks(user_id="no_tasks_user_id")
    assert len(no_tasks_user) == 0

    task_core.create_task("user_with_one_task", "A task")
    no_match_status = task_core.list_tasks(user_id="user_with_one_task", status="done")
    assert len(no_match_status) == 0

    no_match_tag = task_core.list_tasks(
        user_id="user_with_one_task", project_tag="nonexistent"
    )
    assert len(no_match_tag) == 0

    no_match_date = task_core.list_tasks(
        user_id="user_with_one_task",
        due_date_start=datetime.now(timezone.utc) + timedelta(days=100),
    )
    assert len(no_match_date) == 0


def test_reminder_scheduling_and_trigger():
    """Ensure reminders are recorded and triggered."""
    now = datetime.now(timezone.utc)
    due = now + timedelta(hours=2)
    task = task_core.create_task(
        "u1", "Reminder task", due_date_utc=due, reminder_offset_minutes=60
    )
    assert str(task.id) in task_core._reminder_schedule
    # Fast forward past reminder time
    mock_client = type("MC", (), {"send_notification": lambda self, payload: payload})()
    triggered = task_core.check_and_trigger_reminders(
        due - timedelta(minutes=59), mock_client
    )
    assert str(task.id) in triggered
    assert str(task.id) not in task_core._reminder_schedule


def test_update_task_clears_reminder_when_due_date_removed():
    now = datetime.now(timezone.utc)
    due = now + timedelta(hours=3)
    task = task_core.create_task("u2", "Task with reminder", due_date_utc=due)
    assert str(task.id) in task_core._reminder_schedule

    updated = task_core.update_task(str(task.id), updates={"due_date_utc": None})

    assert updated.due_date_utc is None
    assert str(task.id) not in task_core._reminder_schedule
    assert updated.reminder_time_utc is None


def test_create_task_sets_completed_at_when_done():
    task = task_core.create_task("u1", "done task", status="done")
    assert task.completed_at is not None
    assert task.status == "done"


def test_update_task_completed_at_transitions():
    task = task_core.create_task("u1", "toggle status")
    assert task.completed_at is None

    updated = task_core.update_task(str(task.id), updates={"status": "done"})
    assert updated.status == "done"
    assert updated.completed_at is not None

    updated_back = task_core.update_task(
        str(task.id), updates={"status": "in-progress"}
    )
    assert updated_back.completed_at is None


def test_update_task_accepts_reminder_offset_minutes():
    due = datetime.now(timezone.utc) + timedelta(hours=4)
    task = task_core.create_task("u1", "offset test", due_date_utc=due)

    updated = task_core.update_task(
        str(task.id),
        updates={"reminder_offset_minutes": 30, "due_date_utc": due},
    )

    assert str(task.id) in task_core._reminder_schedule
    assert updated.reminder_time_utc == due - timedelta(minutes=30)


# --- End of Tests ---
