from typing import Optional, List, Dict
from datetime import datetime
from orchestrator_agent.calendar_adapters.base_adapter import CalendarAdapter, CalendarEvent # Assuming CalendarEvent is accessible

class MockCalendarAdapter(CalendarAdapter):
    """
    Mock implementation of the CalendarAdapter for integration testing TaskAgent.
    """

    def __init__(self):
        self.events: Dict[str, CalendarEvent] = {}  # Stores events by floe_event_id
        self._connected = False

    def connect(self) -> bool:
        self._connected = True
        return True

    def create_event(self, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> Optional[str]:
        if not self._connected:
            raise RuntimeError("MockCalendarAdapter not connected.")
        if event_data.event_id in self.events:
            # This case should ideally not happen if event_ids are unique for new events
            return None # Or raise an error indicating ID conflict

        self.events[event_data.event_id] = event_data
        return event_data.event_id

    def get_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> Optional[CalendarEvent]:
        if not self._connected:
            raise RuntimeError("MockCalendarAdapter not connected.")
        return self.events.get(floe_event_id)

    def update_event(self, floe_event_id: str, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> bool:
        if not self._connected:
            raise RuntimeError("MockCalendarAdapter not connected.")
        if floe_event_id not in self.events:
            return False
        if floe_event_id != event_data.event_id:
            # Ensure the event_data being passed is meant to update the event identified by floe_event_id
            # This might indicate a mismatch in how the update is called or data prepared.
            # Depending on strictness, could raise ValueError or simply return False.
            print(f"Warning/Error: Mismatched floe_event_id ('{floe_event_id}') and event_data.event_id ('{event_data.event_id}') in update_event.")
            return False

        self.events[floe_event_id] = event_data
        return True

    def delete_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> bool:
        if not self._connected:
            raise RuntimeError("MockCalendarAdapter not connected.")
        if floe_event_id in self.events:
            del self.events[floe_event_id]
            return True
        return False

    def list_events(self,
                    calendar_target: Optional[str] = None,
                    time_min: Optional[datetime] = None,
                    time_max: Optional[datetime] = None,
                    floe_task_id: Optional[str] = None) -> List[CalendarEvent]:
        if not self._connected:
            raise RuntimeError("MockCalendarAdapter not connected.")

        filtered_events = list(self.events.values())

        if time_min:
            filtered_events = [event for event in filtered_events if event.start_time >= time_min]

        if time_max:
            # Assuming event.end_time should be considered for time_max
            filtered_events = [event for event in filtered_events if event.end_time <= time_max]

        if floe_task_id:
            filtered_events = [event for event in filtered_events if event.task_id_ref == floe_task_id]

        return filtered_events

    def clear_events(self):
        """Helper method for tests to reset state."""
        self.events.clear()

    def is_connected(self) -> bool:
        return self._connected
