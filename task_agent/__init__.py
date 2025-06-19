# task_agent/__init__.py
from .task_core import TaskItem
from .task_calendar_linker import (
    TaskInput,
    CalendarEvent,
    create_calendar_event_from_task,
    # add_event_to_calendar_api, # Typically not exported unless truly public API
    # block_time_with_schedule_agent # Typically not exported
)

__all__ = [
    "TaskItem",
    "TaskInput",
    "CalendarEvent",
    "create_calendar_event_from_task",
]
