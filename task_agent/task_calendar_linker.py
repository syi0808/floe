from datetime import datetime, timedelta
from typing import Optional, List  # Renamed List
import uuid  # For generating unique event IDs if not using task_id based

from pydantic import BaseModel, Field, validator

# Assuming CalendarAdapter is in this location. Adjust if moved.
from orchestrator_agent.calendar_adapters.base_adapter import CalendarAdapter
from schedule_agent.schedule_agent import ScheduleAgent

# To use the factory (optional, can be done by client code)
# from orchestrator_agent.calendar_adapters import get_calendar_adapter


class TaskInput(BaseModel):
    """
    Represents the input data for a task, which might come from parsing user input
    or from another system.
    """

    task_id: str = Field(..., description="Unique identifier for the task.")
    description: str = Field(..., description="Full description of the task.")
    start_time: datetime = Field(..., description="Proposed start time for the task.")
    duration_minutes: int = Field(
        ..., gt=0, description="Duration of the task in minutes."
    )
    summary: Optional[str] = Field(
        None,
        description="Optional short summary for the calendar event. If None, derived from description.",
    )


class CalendarEvent(BaseModel):
    """
    Represents a calendar event derived from a task, used for communication with CalendarAdapters.
    """

    event_id: str = Field(
        ..., description="Unique Floe internal identifier for the calendar event."
    )
    summary: str = Field(..., description="Summary or title of the calendar event.")
    start_time: datetime = Field(..., description="Start time of the event.")
    end_time: datetime = Field(..., description="End time of the event.")
    description: Optional[str] = Field(
        None, description="Detailed description for the calendar event."
    )
    task_id_ref: Optional[str] = Field(
        None, description="Optional reference to the original Floe task ID."
    )

    @validator("end_time")
    def end_time_must_be_after_start_time(cls, v, values):
        if "start_time" in values and v <= values["start_time"]:
            raise ValueError("End time must be after start time.")
        return v


# This function can remain standalone or be a static method of TaskCalendarLinker
def create_calendar_event_from_task(
    task_data: TaskInput, base_event_id: Optional[str] = None
) -> CalendarEvent:
    """
    Creates a CalendarEvent object from task details.

    Args:
        task_data: A TaskInput Pydantic model containing task details.
        base_event_id: Optional. If provided, this ID is used for the CalendarEvent's event_id.
                       Otherwise, a new ID is generated based on the task_id.
                       This is useful for updates to preserve the original event_id.
    Returns:
        A CalendarEvent Pydantic model instance.
    """
    if not isinstance(task_data, TaskInput):
        raise TypeError("task_data must be an instance of TaskInput.")

    event_summary = (
        task_data.summary if task_data.summary else task_data.description[:100]
    )
    calculated_end_time = task_data.start_time + timedelta(
        minutes=task_data.duration_minutes
    )

    # Use base_event_id if provided (for updates), otherwise generate new one based on task_id.
    # The original pattern was f"cal_{task_data.task_id}".
    # Using UUIDs for new events ensures uniqueness if task_ids might not be globally unique for events.
    # For this refactor, let's ensure event_id is always robustly unique for new events.
    # If base_event_id is provided, it means we are likely updating an existing event, so use that.
    # If not, it's a new event. Using f"cal_{task_data.task_id}" ties it to task, but if a task can have multiple calendar entries,
    # or if task_id is not guaranteed unique, UUID is better.
    # The protocol implies floe_event_id is our *internal* ID. Adapters will map this to their native IDs or store it.

    current_event_id: str
    if base_event_id:
        current_event_id = base_event_id
    else:
        # Let's use a UUID for new calendar events to ensure they are unique,
        # independent of task_id structure or uniqueness.
        # The old f"cal_{task_data.task_id}" might be okay if a task has only one calendar entry.
        # For more flexibility, UUID is safer.
        current_event_id = str(uuid.uuid4())

    event = CalendarEvent(
        event_id=current_event_id,
        summary=event_summary,
        start_time=task_data.start_time,
        end_time=calculated_end_time,
        description=task_data.description,
        task_id_ref=task_data.task_id,  # This links back to the Floe Task
    )
    return event


def block_time_for_task_via_schedule_agent(
    schedule_agent: ScheduleAgent,
    user_id: str,
    task_input: TaskInput,
    participants: Optional[List[str]] | None = None,
) -> Optional[str]:
    """Create a calendar event for ``task_input`` using ``ScheduleAgent``.

    Parameters mirror :class:`TaskInput`. ``participants`` defaults to ``user_id``.
    Returns the event ID on success or ``None`` otherwise.
    """
    entities = {
        "title": task_input.summary or task_input.description[:50],
        "participants": participants or [user_id],
        "time": task_input.start_time.strftime("%H:%M"),
        "date": task_input.start_time.strftime("%Y-%m-%d"),
        "description": task_input.description,
    }
    resp = schedule_agent.process(entities, user_id)
    if resp.get("status") == "success":
        return resp["data"].get("event_id")
    return None


class TaskCalendarLinker:
    def __init__(self, adapter: CalendarAdapter):
        if not isinstance(
            adapter, CalendarAdapter
        ):  # Check protocol conformance at runtime if desired
            raise TypeError("Adapter must conform to the CalendarAdapter protocol.")
        self.adapter = adapter
        self._connected = False  # Connection status

    def connect_calendar(self) -> bool:
        """
        Connects to the calendar backend using the provided adapter.
        Must be called before other operations.
        """
        try:
            self._connected = self.adapter.connect()
            if self._connected:
                print("TaskCalendarLinker: Successfully connected to calendar adapter.")
            else:
                print("TaskCalendarLinker: Failed to connect to calendar adapter.")
        except Exception as e:
            print(f"TaskCalendarLinker: Error during calendar connection: {e}")
            self._connected = False
        return self._connected

    def add_task_to_calendar(
        self, task_input: TaskInput, calendar_target: Optional[str] = None
    ) -> Optional[str]:
        """
        Creates a calendar event from a task and adds it to the calendar.

        Args:
            task_input: The TaskInput data.
            calendar_target: Optional calendar identifier for the adapter.

        Returns:
            The floe_event_id of the created calendar event, or None if creation failed.
        """
        if not self._connected:
            raise RuntimeError(
                "Calendar adapter not connected. Call connect_calendar() first."
            )

        # create_calendar_event_from_task generates a new event_id (UUID)
        calendar_event = create_calendar_event_from_task(task_input)

        created_floe_event_id = self.adapter.create_event(
            calendar_event, calendar_target
        )
        if (
            created_floe_event_id == calendar_event.event_id
        ):  # Ensure adapter confirmed with our ID
            return calendar_event.event_id
        else:
            # This case implies an issue with the adapter's create_event implementation
            # if it's supposed to return our ID but doesn't, or returns something else on failure.
            # The protocol specifies it returns our internal floe_event_id.
            print(
                f"Warning: Adapter did not confirm creation with expected Floe Event ID. Expected {calendar_event.event_id}, got {created_floe_event_id}"
            )
            return created_floe_event_id  # Return what adapter gave, could be None

    def get_linked_event(
        self, floe_event_id: str, calendar_target: Optional[str] = None
    ) -> Optional[CalendarEvent]:
        """Retrieves a calendar event by its Floe event ID."""
        if not self._connected:
            raise RuntimeError(
                "Calendar adapter not connected. Call connect_calendar() first."
            )
        return self.adapter.get_event(floe_event_id, calendar_target)

    def update_linked_event(
        self,
        floe_event_id: str,
        task_input: TaskInput,
        calendar_target: Optional[str] = None,
    ) -> bool:
        """
        Updates an existing calendar event linked to a task.
        The floe_event_id identifies the event to update.
        task_input provides the new data for the event.
        """
        if not self._connected:
            raise RuntimeError(
                "Calendar adapter not connected. Call connect_calendar() first."
            )

        # Re-create CalendarEvent using the task_input, but ensure it uses the existing floe_event_id
        calendar_event_update_data = create_calendar_event_from_task(
            task_input, base_event_id=floe_event_id
        )

        return self.adapter.update_event(
            floe_event_id, calendar_event_update_data, calendar_target
        )

    def remove_task_from_calendar(
        self, floe_event_id: str, calendar_target: Optional[str] = None
    ) -> bool:
        """Removes a calendar event by its Floe event ID."""
        if not self._connected:
            raise RuntimeError(
                "Calendar adapter not connected. Call connect_calendar() first."
            )
        return self.adapter.delete_event(floe_event_id, calendar_target)

    def list_linked_events(
        self,
        calendar_target: Optional[str] = None,
        time_min: Optional[datetime] = None,
        time_max: Optional[datetime] = None,
        floe_task_id: Optional[str] = None,
    ) -> List[CalendarEvent]:
        """Lists calendar events, with optional filters."""
        if not self._connected:
            raise RuntimeError(
                "Calendar adapter not connected. Call connect_calendar() first."
            )
        return self.adapter.list_events(
            calendar_target, time_min, time_max, floe_task_id
        )

    def block_time_for_task(
        self,
        schedule_agent: ScheduleAgent,
        user_id: str,
        task_input: TaskInput,
        participants: Optional[List[str]] | None = None,
        calendar_target: Optional[str] = None,
    ) -> Optional[str]:
        """Use ``ScheduleAgent`` to reserve time for ``task_input`` and record the event.

        The event returned by ``ScheduleAgent`` is created via the connected calendar
        adapter so that the task appears on the user's calendar. The resulting event
        ID is returned on success or ``None`` on failure.
        """

        if not self._connected:
            raise RuntimeError(
                "Calendar adapter not connected. Call connect_calendar() first."
            )

        floe_event_id = block_time_for_task_via_schedule_agent(
            schedule_agent,
            user_id,
            task_input,
            participants,
        )

        if not floe_event_id:
            return None

        event = create_calendar_event_from_task(task_input, base_event_id=floe_event_id)
        created_id = self.adapter.create_event(event, calendar_target)

        if created_id != floe_event_id:
            print(
                f"Warning: Adapter did not confirm creation with expected Floe Event ID. Expected {floe_event_id}, got {created_id}"
            )
            return created_id

        return floe_event_id


if __name__ == "__main__":
    print("TaskCalendarLinker module - Basic Structure Test")
    print(
        "This main block is for demonstrating structure, not full functionality without a real adapter."
    )

    # Example of using a dummy adapter that conforms to the protocol
    class DummyCalendarAdapter(CalendarAdapter):
        def connect(self) -> bool:
            print("DummyAdapter: connect() called")
            return True

        def create_event(
            self, event_data: CalendarEvent, calendar_target: Optional[str] = None
        ) -> Optional[str]:
            print(
                f"DummyAdapter: create_event called for Floe ID {event_data.event_id} in '{calendar_target or 'default'}'"
            )
            return event_data.event_id  # Simulate successful creation

        def get_event(
            self, floe_event_id: str, calendar_target: Optional[str] = None
        ) -> Optional[CalendarEvent]:
            print(f"DummyAdapter: get_event called for Floe ID {floe_event_id}")
            # Simulate finding an event
            return CalendarEvent(
                event_id=floe_event_id,
                summary="Dummy Event",
                start_time=datetime.now(),
                end_time=datetime.now() + timedelta(hours=1),
                task_id_ref="dummy_task_001",
            )

        def update_event(
            self,
            floe_event_id: str,
            event_data: CalendarEvent,
            calendar_target: Optional[str] = None,
        ) -> bool:
            print(f"DummyAdapter: update_event called for Floe ID {floe_event_id}")
            return True

        def delete_event(
            self, floe_event_id: str, calendar_target: Optional[str] = None
        ) -> bool:
            print(f"DummyAdapter: delete_event called for Floe ID {floe_event_id}")
            return True

        def list_events(
            self,
            calendar_target: Optional[str] = None,
            time_min: Optional[datetime] = None,
            time_max: Optional[datetime] = None,
            floe_task_id: Optional[str] = None,
        ) -> List[CalendarEvent]:
            print(f"DummyAdapter: list_events called for task ID {floe_task_id}")
            return []

    dummy_adapter = DummyCalendarAdapter()
    linker = TaskCalendarLinker(adapter=dummy_adapter)

    if linker.connect_calendar():
        print("\nLinker connected using DummyAdapter.")
        sample_task = TaskInput(
            task_id="task_dummy_001",
            description="This is a sample task for the dummy linker.",
            summary="Dummy Task Event",
            start_time=datetime.now() + timedelta(days=1),
            duration_minutes=60,
        )

        print("\nAttempting to add task to calendar...")
        new_event_id = linker.add_task_to_calendar(sample_task, "dummy_calendar")
        if new_event_id:
            print(f"Task added, new Floe Event ID: {new_event_id}")

            print("\nAttempting to get linked event...")
            event = linker.get_linked_event(new_event_id, "dummy_calendar")
            if event:
                print(f"Retrieved event: {event.summary}")

            print("\nAttempting to update linked event...")
            linker.update_linked_event(new_event_id, sample_task, "dummy_calendar")

            print("\nAttempting to list linked events for the task...")
            linker.list_linked_events(floe_task_id="task_dummy_001")

            print("\nAttempting to remove task from calendar...")
            linker.remove_task_from_calendar(new_event_id, "dummy_calendar")
        else:
            print("Failed to add task to calendar via dummy adapter.")
    else:
        print("Linker failed to connect using DummyAdapter.")
