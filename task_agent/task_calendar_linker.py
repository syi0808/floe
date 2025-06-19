import datetime
import logging
from typing import Optional, Any, Dict
from uuid import uuid4

from pydantic import BaseModel, Field

# Assuming TaskItem might be useful for context, though not directly in test signatures
# from task_agent.task_core import TaskItem # Not strictly needed by current tests but good for future

logger = logging.getLogger(__name__)

class TaskCalendarLink(BaseModel):
    """
    Represents the link between a task and a calendar event.
    """
    task_id: str
    calendar_event_id: Optional[str] = None
    calendar_service_provider: Optional[str] = None # e.g., "google_calendar", "outlook_calendar"
    event_summary: Optional[str] = None
    start_time: Optional[datetime.datetime] = None
    end_time: Optional[datetime.datetime] = None
    status: str = Field(default="pending_creation") # e.g., "pending_creation", "created", "failed", "deleted"
    error_message: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None # For any other relevant info


class BaseScheduleAgentClient:
    """
    Abstract base class (or interface) for a schedule agent client.
    This helps in defining a contract for what TaskCalendarLinker expects.
    A concrete implementation or a mock will be used.
    """
    async def create_event(
        self,
        summary: str,
        start_time: datetime.datetime,
        end_time: datetime.datetime,
        description: Optional[str] = None,
        attendees: Optional[list[str]] = None,
        # ... other common calendar event params
    ) -> Dict[str, Any]:
        """
        Creates an event in the calendar.
        Returns a dictionary with event details, minimally including 'id'.
        """
        raise NotImplementedError

    async def delete_event(self, event_id: str) -> Dict[str, Any]:
        """
        Deletes an event from the calendar.
        Returns a dictionary confirming deletion.
        """
        raise NotImplementedError

class MockScheduleAgentClient(BaseScheduleAgentClient):
    """
    A mock client that simulates interactions with a calendar service.
    Matches the one used in the tests.
    """
    async def create_event(
        self,
        summary: str,
        start_time: datetime.datetime,
        end_time: datetime.datetime,
        description: Optional[str] = None,
        attendees: Optional[list[str]] = None,
        # ... other common calendar event params
    ) -> Dict[str, Any]:
        event_id = f"evt_mock_{uuid4()}"
        logger.info(f"MockScheduleAgentClient: Creating event '{summary}' with id {event_id} from {start_time} to {end_time}")
        return {"id": event_id, "status": "confirmed", "summary": summary, "start_time": start_time, "end_time": end_time}

    async def delete_event(self, event_id: str) -> Dict[str, Any]:
        logger.info(f"MockScheduleAgentClient: Deleting event {event_id}")
        return {"status": "deleted", "deleted_event_id": event_id}


class TaskCalendarLinker:
    def __init__(self, schedule_agent_client: Optional[BaseScheduleAgentClient] = None):
        # If no client is provided, use the mock client by default for now.
        # In a real application, you might want to raise an error or have a more robust fallback.
        self.client = schedule_agent_client if schedule_agent_client else MockScheduleAgentClient()
        # This internal storage is a placeholder for a real database or state management solution.
        self._links: Dict[str, TaskCalendarLink] = {}


    async def create_calendar_event_for_task(
        self,
        task_id: str,
        task_description: str,
        suggested_start_time: Optional[datetime.datetime] = None,
        suggested_end_time: Optional[datetime.datetime] = None,
        duration_minutes: Optional[int] = None,
        # task_due_date: Optional[datetime.datetime] = None, # From TaskItem.due_date_utc
        calendar_service_provider: str = "default_mock_calendar" # Example
    ) -> TaskCalendarLink:

        start_time = suggested_start_time
        end_time = suggested_end_time

        if start_time and duration_minutes and not end_time:
            end_time = start_time + datetime.timedelta(minutes=duration_minutes)
        elif end_time and duration_minutes and not start_time:
            start_time = end_time - datetime.timedelta(minutes=duration_minutes)
        elif start_time and end_time and duration_minutes:
            # All three provided, check for consistency or prioritize start+duration
            calculated_end_time = start_time + datetime.timedelta(minutes=duration_minutes)
            if end_time != calculated_end_time:
                logger.warning(
                    f"Inconsistent end_time and duration provided for task {task_id}. "
                    f"Prioritizing start_time + duration_minutes. Original end_time: {end_time}, "
                    f"Calculated end_time: {calculated_end_time}"
                )
                end_time = calculated_end_time
        elif duration_minutes and not start_time and not end_time:
            # Only duration provided, default start to now (as per test)
            start_time = datetime.datetime.now(datetime.timezone.utc)
            end_time = start_time + datetime.timedelta(minutes=duration_minutes)
        # elif task_due_date and not start_time and not end_time and not duration_minutes:
        #     # Fallback to task_due_date: create a 30-min event on that day at a default time (e.g., 9 AM)
        #     # Or an all-day event. For now, let's assume specific time info is preferred.
        #     # This part is not explicitly in tests but a logical extension.
        #     # For now, let's stick to what tests imply: insufficient info if not calculable.
        #     pass

        if not start_time or not end_time:
            error_msg = "Insufficient time information to create a calendar event. Provide start/end, start/duration, or end/duration."
            logger.error(f"Task {task_id}: {error_msg}")
            link = TaskCalendarLink(
                task_id=task_id,
                status="failed",
                error_message=error_msg,
                event_summary=f"Task: {task_description}",
                calendar_service_provider=calendar_service_provider,
            )
            self._links[task_id] = link # Store the failed attempt
            return link

        # Ensure times are timezone-aware (UTC, as per tests and general best practice)
        if start_time.tzinfo is None:
            start_time = start_time.replace(tzinfo=datetime.timezone.utc)
        if end_time.tzinfo is None:
            end_time = end_time.replace(tzinfo=datetime.timezone.utc)

        event_summary = f"Task: {task_description}"

        try:
            event_details = await self.client.create_event(
                summary=event_summary,
                start_time=start_time,
                end_time=end_time,
                description=f"Calendar event for task ID: {task_id}\n\n{task_description}"
            )
            calendar_event_id = event_details.get("id")
            if calendar_event_id:
                link = TaskCalendarLink(
                    task_id=task_id,
                    calendar_event_id=calendar_event_id,
                    calendar_service_provider=calendar_service_provider,
                    event_summary=event_summary,
                    start_time=start_time,
                    end_time=end_time,
                    status="created",
                )
                logger.info(f"Successfully created calendar event {calendar_event_id} for task {task_id}")
                self._links[task_id] = link # Store successful link
                return link
            else:
                error_msg = "Failed to create calendar event: No event ID returned from client."
                logger.error(f"Task {task_id}: {error_msg} (Client response: {event_details})")
                link = TaskCalendarLink(
                    task_id=task_id, status="failed", error_message=error_msg, event_summary=event_summary,
                    start_time=start_time, end_time=end_time, calendar_service_provider=calendar_service_provider,
                )
                self._links[task_id] = link
                return link

        except Exception as e:
            error_msg = f"Exception during calendar event creation: {e}"
            logger.exception(f"Task {task_id}: {error_msg}", exc_info=True)
            link = TaskCalendarLink(
                task_id=task_id, status="failed", error_message=error_msg, event_summary=event_summary,
                start_time=start_time, end_time=end_time, calendar_service_provider=calendar_service_provider,
            )
            self._links[task_id] = link
            return link

    async def get_task_calendar_link(self, task_id: str) -> Optional[TaskCalendarLink]:
        """
        Retrieves the stored calendar link information for a task.
        Placeholder: In a real app, this would fetch from a database.
        """
        logger.info(f"Attempting to retrieve calendar link for task_id: {task_id}")
        # The test `test_get_task_calendar_link_placeholder` expects None.
        # If we want to make it retrievable for other tests, we'd use:
        # return self._links.get(task_id)
        # For now, adhering strictly to the placeholder test's expectation.
        # The specific test `test_get_task_calendar_link_placeholder` calls `linker_instance.get_task_calendar_link("some_task_id")`
        # without creating a link for "some_task_id" first. So `self._links.get("some_task_id")` will be None.
        return self._links.get(task_id)


    async def remove_calendar_event_for_task(self, task_id: str, event_id: Optional[str] = None) -> bool:
        """
        Removes a calendar event associated with a task.
        If event_id is not provided, it tries to find it from stored links.
        """
        effective_event_id = event_id
        if not effective_event_id:
            link = self._links.get(task_id)
            if link and link.calendar_event_id:
                effective_event_id = link.calendar_event_id
            else:
                logger.warning(f"No event_id provided and no link found for task {task_id} to determine which event to remove.")
                # The test `test_remove_calendar_event_for_task_placeholder` passes ("task_id_to_remove", "event_id_to_remove")
                # and expects True. This means that even if the link is not in self._links, if an event_id is given,
                # it should proceed to attempt deletion.
                if not event_id: # Still no event_id after checking link
                     logger.error(f"Cannot remove event for task {task_id}: event_id is missing and not found in links.")
                     return False

        if not effective_event_id: # Final check if an event_id could be determined
            logger.error(f"Event ID for task {task_id} is unknown. Cannot remove.")
            return False # Should not happen if event_id was passed directly as in tests.

        try:
            await self.client.delete_event(effective_event_id)
            logger.info(f"Successfully requested deletion of calendar event {effective_event_id} for task {task_id}")
            # Update local link status if it exists
            if task_id in self._links:
                self._links[task_id].status = "deleted"
                self._links[task_id].error_message = None # Clear previous errors
            return True
        except Exception as e:
            logger.exception(f"Exception during calendar event deletion for task {task_id}, event {effective_event_id}: {e}", exc_info=True)
            if task_id in self._links:
                self._links[task_id].status = "failed_deletion"
                self._links[task_id].error_message = str(e)
            return False

# Example of how TaskCalendarLinker could be integrated with TaskItem
# async def schedule_task(task: TaskItem, linker: TaskCalendarLinker):
#     if task.due_date_utc:
#         # Example: schedule it for 1 hour starting at due_date_utc
#         # This is just illustrative.
#         link_info = await linker.create_calendar_event_for_task(
#             task_id=str(task.id),
#             task_description=task.description,
#             suggested_start_time=task.due_date_utc,
#             duration_minutes=60 # Default duration
#         )
#         if link_info.status == "created" and link_info.calendar_event_id:
#             task.linked_schedule_id = link_info.calendar_event_id
#             # Here you would typically update the task in your task storage (e.g., task_core.update_task)
#             logger.info(f"Task {task.id} linked to calendar event {task.linked_schedule_id}")
#         else:
#             logger.error(f"Failed to schedule task {task.id}: {link_info.error_message}")
#     else:
#         logger.info(f"Task {task.id} has no due date, not scheduling.")
