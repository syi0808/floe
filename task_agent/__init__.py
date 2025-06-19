# task_agent/__init__.py
from .task_core import TaskItem
from .task_calendar_linker import (
    TaskInput,
    CalendarEvent,
    create_calendar_event_from_task, # This is still a useful utility function
    TaskCalendarLinker
)

__all__ = [
    "TaskItem",
    "TaskInput",
    "CalendarEvent",
    "create_calendar_event_from_task",
    "TaskCalendarLinker",
]
