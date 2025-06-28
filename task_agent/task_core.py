from datetime import datetime, timezone, timedelta
from typing import List, Optional, Literal, Dict, Any
from uuid import UUID, uuid4

from pydantic import BaseModel, Field, ValidationError

# Alias for valid task statuses used across the codebase.
# Support both "in progress" with a space and the original "in-progress" form
# for backward compatibility.
TaskStatus = Literal['todo', 'in-progress', 'in progress', 'done', 'archived']

# In-memory storage for tasks
_task_storage: Dict[str, 'TaskItem'] = {}  # Key: str(task.id) -> TaskItem
_reminder_schedule: Dict[str, datetime] = {}
DEFAULT_REMINDER_MINUTES = 1440  # 24 hours before due date

# Basic priority calculation can be improved in the future (e.g. Eisenhower
# matrix).  For now ``_calculate_priority`` derives a score from the due date if
# a priority is not explicitly provided.

def _calculate_priority(due_date_utc: Optional[datetime]) -> int:
    """Return a priority from ``1`` (highest) to ``4`` based on ``due_date_utc``.

    If ``due_date_utc`` is ``None`` the default priority ``2`` is returned.  The
    closer the due date is, the higher the priority score.
    """
    if due_date_utc is None:
        return 2
    now = datetime.now(timezone.utc)
    delta = due_date_utc - now
    if delta <= timedelta(days=1):
        return 1
    if delta <= timedelta(days=3):
        return 2
    if delta <= timedelta(days=7):
        return 3
    return 4

class TaskItem(BaseModel):
    id: UUID = Field(default_factory=uuid4)
    user_id: str
    description: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    due_date_utc: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    priority: int = Field(default=2, ge=1, le=4)  # 1=Highest, 4=Lowest
    status: TaskStatus = 'todo'
    project_tags: Optional[List[str]] = None
    linked_schedule_id: Optional[str] = None
    reminder_time_utc: Optional[datetime] = None

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
    priority: Optional[int] = None,
    project_tags: Optional[List[str]] = None,
    status: TaskStatus = 'todo',
    reminder_offset_minutes: Optional[int] = None,
) -> TaskItem:
    computed_priority = priority if priority is not None else _calculate_priority(due_date_utc)
    task = TaskItem(
        user_id=user_id,
        description=description,
        due_date_utc=due_date_utc,
        priority=computed_priority,
        project_tags=project_tags,
        status=status
    )
    _task_storage[str(task.id)] = task

    # Automatically schedule reminder if due date is set
    if due_date_utc:
        offset = (
            reminder_offset_minutes
            if reminder_offset_minutes is not None
            else DEFAULT_REMINDER_MINUTES
        )
        schedule_reminder(task, offset)
    return task

def get_task(task_id: str, user_id: Optional[str] = None) -> TaskItem:
    task = _task_storage.get(task_id)
    if not task or (user_id is not None and task.user_id != user_id):
        raise ValueError(f"Task with ID {task_id} not found")
    return task

def update_task(task_id: str, user_id: Optional[str], updates: Dict[str, Any]) -> TaskItem:
    task = get_task(task_id, user_id)

    task_data = task.model_dump()
    IMMUTABLE = {"id", "created_at", "user_id"}
    filtered_updates = {k: v for k, v in updates.items() if k not in IMMUTABLE}

    try:
        if "priority" not in filtered_updates and "due_date_utc" in filtered_updates:
            filtered_updates["priority"] = _calculate_priority(filtered_updates["due_date_utc"])
        updated_task = TaskItem(**{**task_data, **filtered_updates})
    except ValidationError as e:
        raise ValueError(str(e))

    _task_storage[task_id] = updated_task

    if updated_task.due_date_utc:
        offset = updates.get("reminder_offset_minutes", DEFAULT_REMINDER_MINUTES)
        schedule_reminder(updated_task, offset)

    return updated_task

def delete_task(task_id: str, user_id: Optional[str] = None) -> bool:
    task = get_task(task_id, user_id)
    del _task_storage[task_id]
    _reminder_schedule.pop(task_id, None)
    return True

def list_tasks(
    user_id: str,
    status: Optional[TaskStatus] = None,
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

# --- Reminder Scheduling -------------------------------------------------

def schedule_reminder(task: TaskItem, offset_minutes: int) -> None:
    """Record a reminder time for ``task``.

    The actual notification dispatch will be integrated with MCP in the future.
    """
    if not task.due_date_utc:
        return
    reminder_time = task.due_date_utc - timedelta(minutes=offset_minutes)
    task.reminder_time_utc = reminder_time
    _reminder_schedule[str(task.id)] = reminder_time


def check_and_trigger_reminders(now: datetime, mcp_client: Optional[Any] = None) -> List[str]:
    """Check for due reminders and optionally notify via ``mcp_client``.

    Returns a list of task IDs for which reminders were triggered.
    """
    triggered: List[str] = []
    for task_id, remind_at in list(_reminder_schedule.items()):
        if remind_at <= now:
            task = _task_storage.get(task_id)
            if task and mcp_client:
                try:
                    mcp_client.send_notification(
                        {
                            "user_id": task.user_id,
                            "type": "task_reminder",
                            "task_id": task_id,
                            "message": f"Reminder: {task.description} is due soon",
                        }
                    )
                except Exception:
                    pass
            triggered.append(task_id)
            _reminder_schedule.pop(task_id, None)
    return triggered

