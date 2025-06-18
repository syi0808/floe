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
        # Prevent overwriting immutable fields
        IMMUTABLE = {"id", "created_at", "user_id"}
        filtered_updates = {k: v for k, v in updates.items() if k not in IMMUTABLE}
        updated_task_data = {**task_data, **filtered_updates}

        try:
            # Create a new TaskItem instance from the merged data
            # This validates all fields, including those not explicitly in 'updates'
            updated_task = TaskItem(**updated_task_data)
            _task_storage[task_id] = updated_task
            return updated_task
        except ValidationError:
            return None  # Invalid data based on model validation rules
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
