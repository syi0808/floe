from datetime import datetime, timezone
from typing import List, Optional, Literal, Dict, Any # Added Any
from uuid import UUID, uuid4

from pydantic import BaseModel, Field, ValidationError # Added ValidationError

# In-memory storage for tasks
_task_storage: Dict[str, 'TaskItem'] = {} # Key: str(task.id), Value: TaskItem instance

# Future enhancement: Implement more advanced priority calculation,
# e.g., based on Eisenhower matrix (Urgent/Important) or due date proximity.
# Currently, priority is set directly upon task creation.

class TaskItem(BaseModel):
    id: UUID = Field(default_factory=uuid4)
    user_id: str
    description: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    due_date_utc: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    priority: int = Field(default=2, ge=1, le=4)  # 1=Highest, 4=Lowest
    status: Literal['todo', 'in-progress', 'done', 'archived'] = 'todo'
    project_tags: Optional[List[str]] = None
    linked_schedule_id: Optional[str] = None

    model_config = { # Pydantic V2 style
        "json_encoders": {
            UUID: lambda v: str(v),
            # Normalise UTC datetimes to RFC-3339 “Z” for consistency with
            # most JSON APIs and test expectations.
            datetime: lambda v: v.isoformat().replace("+00:00", "Z") if v else None
        }
    }

# --- CRUD Operations ---

def create_task(
    user_id: str,
    description: str,
    due_date_utc: Optional[datetime] = None,
    priority: int = 2, # Default priority from TaskItem
    project_tags: Optional[List[str]] = None,
    status: Literal['todo', 'in-progress', 'done', 'archived'] = 'todo' # Default status
) -> TaskItem:
    task = TaskItem(
        user_id=user_id,
        description=description,
        due_date_utc=due_date_utc,
        priority=priority,
        project_tags=project_tags,
        status=status
    )
    _task_storage[str(task.id)] = task
    return task

def get_task(task_id: str) -> Optional[TaskItem]:
    return _task_storage.get(task_id)

def update_task(task_id: str, updates: Dict[str, Any]) -> Optional[TaskItem]:
    task = _task_storage.get(task_id)
    if task:
        # Create a dictionary of the existing task's data
        task_data = task.model_dump()
        # Merge updates into this dictionary
        # This ensures that only specified fields are updated,
        # and unspecified fields retain their original values.
        updated_task_data = {**task_data, **updates}

        try:
            # Create a new TaskItem instance from the merged data
            # This validates all fields, including those not explicitly in 'updates'
            updated_task = TaskItem(**updated_task_data)
            _task_storage[task_id] = updated_task
            return updated_task
        except ValidationError:
            return None # Invalid data based on model validation rules
    return None

def delete_task(task_id: str) -> bool:
    if task_id in _task_storage:
        del _task_storage[task_id]
        return True
    return False

def list_tasks(
    user_id: str,
    status: Optional[Literal['todo', 'in-progress', 'done', 'archived']] = None,
    project_tag: Optional[str] = None,
    due_date_start: Optional[datetime] = None,
    due_date_end: Optional[datetime] = None
) -> List[TaskItem]:
    user_tasks = [task for task in _task_storage.values() if task.user_id == user_id]

    if status:
        user_tasks = [task for task in user_tasks if task.status == status]

    if project_tag:
        user_tasks = [
            task for task in user_tasks
            if task.project_tags and project_tag in task.project_tags
        ]

    if due_date_start:
        user_tasks = [
            task for task in user_tasks
            if task.due_date_utc and task.due_date_utc >= due_date_start
        ]

    if due_date_end:
        user_tasks = [
            task for task in user_tasks
            if task.due_date_utc and task.due_date_utc <= due_date_end
        ]

    return sorted(user_tasks, key=lambda t: (t.priority, t.created_at))

# Future enhancement: Integrate reminder logic.
# This would involve scheduling notifications for tasks with due dates,
# potentially interacting with a central notification service or MCP.

if __name__ == '__main__':
    # Initial TaskItem examples (from previous step)
    task_example1 = TaskItem(user_id="user123", description="Buy groceries")
    print("Initial TaskItem Example 1:")
    print(task_example1.model_dump_json(indent=2))

    task_example2 = TaskItem(
        user_id="user456",
        description="Prepare presentation for Monday",
        due_date_utc=datetime(2024, 7, 1, 9, 0, 0, tzinfo=timezone.utc),
        priority=1,
        project_tags=["project-alpha", "client-meeting"]
    )
    print("\nInitial TaskItem Example 2:")
    print(task_example2.model_dump_json(indent=2))

    print("\n--- CRUD Operations Test ---")

    # Create tasks
    print("\n1. Creating tasks...")
    task1 = create_task(
        user_id="user001",
        description="Schedule dentist appointment",
        priority=1,
        project_tags=["health"]
    )
    task2 = create_task(
        user_id="user001",
        description="Pay electricity bill",
        due_date_utc=datetime(2024, 6, 20, 17, 0, 0, tzinfo=timezone.utc),
        project_tags=["finance", "bills"]
    )
    task3 = create_task(
        user_id="user002",
        description="Grocery shopping for the week",
        status="in-progress"
    )
    print(f"Task 1 created: {task1.id}")
    print(f"Task 2 created: {task2.id}")
    print(f"Task 3 created: {task3.id}")
    print(f"Current storage: {len(_task_storage)} tasks")

    # Get a task
    print("\n2. Getting a task...")
    retrieved_task1 = get_task(str(task1.id))
    if retrieved_task1:
        print(f"Retrieved Task 1: {retrieved_task1.description}, Priority: {retrieved_task1.priority}")
    else:
        print("Task 1 not found.")

    # Update a task
    print("\n3. Updating a task...")
    updated_data = {"status": "in-progress", "priority": 2, "description": "Schedule annual dentist check-up"}
    updated_task1 = update_task(str(task1.id), updated_data)
    if updated_task1:
        print(f"Updated Task 1: {updated_task1.description}, Status: {updated_task1.status}, Priority: {updated_task1.priority}")
        # Verify change in storage
        # print(f"Task 1 in storage: {_task_storage[str(task1.id)].model_dump_json(indent=2)}")

    # Update with invalid data (e.g. wrong priority value)
    print("\nTrying to update with invalid priority (e.g., 5)...")
    invalid_update_data = {"priority": 5} # Priority must be between 1 and 4
    invalid_update_attempt = update_task(str(task1.id), invalid_update_data)
    if not invalid_update_attempt:
        print("Update with invalid priority correctly failed.")
        retrieved_task1_after_failed_update = get_task(str(task1.id))
        if retrieved_task1_after_failed_update:
             print(f"Task 1 priority remains: {retrieved_task1_after_failed_update.priority}")


    # List tasks for user001
    print("\n4. Listing tasks for user001...")
    user001_tasks = list_tasks(user_id="user001")
    for t in user001_tasks:
        print(f"  - {t.description} (Priority: {t.priority}, Status: {t.status}, Due: {t.due_date_utc})")

    # List tasks for user001 with status 'in-progress'
    print("\n5. Listing 'in-progress' tasks for user001...")
    user001_inprogress_tasks = list_tasks(user_id="user001", status="in-progress")
    for t in user001_inprogress_tasks:
        print(f"  - {t.description} (Priority: {t.priority}, Status: {t.status})")

    # List tasks for user001 with project_tag 'health'
    print("\n6. Listing 'health' tasks for user001...")
    user001_health_tasks = list_tasks(user_id="user001", project_tag="health")
    for t in user001_health_tasks:
        print(f"  - {t.description} (Project Tags: {t.project_tags})")

    # Delete a task
    print("\n7. Deleting a task...")
    delete_success = delete_task(str(task2.id))
    print(f"Deletion of Task 2 successful: {delete_success}")
    print(f"Task 2 still in storage: {str(task2.id) in _task_storage}")
    print(f"Current storage: {len(_task_storage)} tasks")

    # Try to get a deleted task
    print("\n8. Trying to get deleted Task 2...")
    deleted_task_retrieval = get_task(str(task2.id))
    if not deleted_task_retrieval:
        print("Task 2 correctly not found after deletion.")

    # List all tasks for user001 again
    print("\n9. Listing tasks for user001 after deletion...")
    user001_tasks_after_delete = list_tasks(user_id="user001")
    for t in user001_tasks_after_delete:
        print(f"  - {t.description} (Status: {t.status})")

    print("\n--- End of CRUD Operations Test ---")
