from .host_agent import HostAgent
from typing import Callable
from common.types import (
    AgentCard,
    Task,
    TaskStatusUpdateEvent,
    TaskArtifactUpdateEvent,
)

TaskCallbackArg = Task | TaskStatusUpdateEvent | TaskArtifactUpdateEvent
TaskUpdateCallback = Callable[[TaskCallbackArg, AgentCard], Task]


def task_callback(
    task: TaskCallbackArg,
    card: AgentCard,
) -> Task:
    """A callback function to handle task updates."""
    print("TASK CALLBACK")
    print(task)
    print(card)

    return task


root_agent = HostAgent(["http://localhost:10001"], task_callback).create_agent()
