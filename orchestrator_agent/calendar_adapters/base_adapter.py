# orchestrator_agent/calendar_adapters/base_adapter.py
from typing import Protocol, Optional, List, runtime_checkable # Renamed List to avoid conflict, Added runtime_checkable
from datetime import datetime

# Assuming CalendarEvent and TaskInput will be accessible from this path.
# This might move to a common_types module later.
try:
    from task_agent.task_calendar_linker import CalendarEvent
except ImportError:
    # Fallback for scenarios where the direct import might fail during development/refactoring
    # This indicates that CalendarEvent might need to be in a more universally accessible location.
    print("Warning: Could not import CalendarEvent directly from task_agent.task_calendar_linker in base_adapter.py. "
          "Ensure it's defined or moved to a common types module if issues persist.")
    # Define a placeholder if not found, to allow protocol definition.
    # This is not ideal for runtime but helps define the interface.
    class CalendarEvent: pass


@runtime_checkable
class CalendarAdapter(Protocol):
    """
    Protocol defining the interface for different calendar backend adapters.
    """

    def connect(self) -> bool:
        """
        Establishes connection or performs setup for the calendar service.
        For example, for Google Calendar, this would involve OAuth and building the service object.
        For local calendars like Apple Calendar, this might just verify prerequisites or always return True.

        Returns:
            bool: True if connection/setup is successful, False otherwise.
        """
        return True # Default implementation

    def create_event(self, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> Optional[str]:
        """
        Creates a new event in the calendar.

        Args:
            event_data: A CalendarEvent object containing the details of the event to create.
                        The event_data.event_id should be the Floe internal ID for this event.
            calendar_target: Optional identifier for the specific calendar/calendar list
                             (e.g., "primary" for Google, calendar name for Apple).

        Returns:
            Optional[str]: The internal Floe event ID (event_data.event_id) if creation was successful,
                           None otherwise.
        """
        ...

    def get_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> Optional[CalendarEvent]:
        """
        Retrieves a specific event by its internal Floe event ID.

        Args:
            floe_event_id: The internal Floe ID of the event to retrieve.
            calendar_target: Optional identifier for the specific calendar.

        Returns:
            Optional[CalendarEvent]: The event object if found, None otherwise.
        """
        ...

    def update_event(self, floe_event_id: str, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> bool:
        """
        Updates an existing event identified by its internal Floe event ID.

        Args:
            floe_event_id: The internal Floe ID of the event to update.
            event_data: A CalendarEvent object with the updated details.
                        The event_data.event_id should match floe_event_id.
            calendar_target: Optional identifier for the specific calendar.

        Returns:
            bool: True if the update was successful, False otherwise.
        """
        ...

    def delete_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> bool:
        """
        Deletes an event identified by its internal Floe event ID.

        Args:
            floe_event_id: The internal Floe ID of the event to delete.
            calendar_target: Optional identifier for the specific calendar.

        Returns:
            bool: True if deletion was successful, False otherwise.
        """
        ...

    def list_events(self,
                    calendar_target: Optional[str] = None,
                    time_min: Optional[datetime] = None,
                    time_max: Optional[datetime] = None,
                    floe_task_id: Optional[str] = None) -> List[CalendarEvent]:
        """
        Lists events from the calendar, with optional filters.

        Args:
            calendar_target: Optional identifier for the specific calendar.
            time_min: Optional start time for filtering events.
            time_max: Optional end time for filtering events.
            floe_task_id: Optional Floe task ID to filter events linked to a specific task.

        Returns:
            List[CalendarEvent]: A list of event objects matching the criteria. Empty list if none found or an error.
        """
        ...
