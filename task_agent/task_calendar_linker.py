from datetime import datetime, timedelta
from typing import Optional, Dict, Any
from pydantic import BaseModel, Field, validator

class TaskInput(BaseModel):
    """
    Represents the input data for a task, which might come from parsing user input
    or from another system.
    """
    task_id: str = Field(..., description="Unique identifier for the task.")
    description: str = Field(..., description="Full description of the task.")
    start_time: datetime = Field(..., description="Proposed start time for the task.")
    duration_minutes: int = Field(..., gt=0, description="Duration of the task in minutes.")
    summary: Optional[str] = Field(None, description="Optional short summary for the calendar event. If None, derived from description.")

class CalendarEvent(BaseModel):
    """
    Represents a calendar event derived from a task.
    """
    event_id: str = Field(..., description="Unique identifier for the calendar event (could be same as task_id or newly generated).")
    summary: str = Field(..., description="Summary or title of the calendar event.")
    start_time: datetime = Field(..., description="Start time of the event.")
    end_time: datetime = Field(..., description="End time of the event.")
    description: Optional[str] = Field(None, description="Detailed description for the calendar event.")
    task_id_ref: str = Field(..., description="Reference to the original task ID.")

    @validator('end_time')
    def end_time_must_be_after_start_time(cls, v, values):
        if 'start_time' in values and v <= values['start_time']:
            raise ValueError('End time must be after start time.')
        return v

def create_calendar_event_from_task(task_data: TaskInput) -> CalendarEvent:
    """
    Creates a CalendarEvent object from task details.

    Args:
        task_data: A TaskInput Pydantic model containing task details.

    Returns:
        A CalendarEvent Pydantic model instance.

    Raises:
        TypeError: If task_data is not an instance of TaskInput.
        ValueError: If task_data contains invalid values (via Pydantic validation).
    """
    if not isinstance(task_data, TaskInput):
        # This check is somewhat redundant if type hints are enforced,
        # but can be useful in environments where they are not.
        raise TypeError("task_data must be an instance of TaskInput.")

    event_summary = task_data.summary if task_data.summary else task_data.description[:100] # Use first 100 chars of description if no summary

    calculated_end_time = task_data.start_time + timedelta(minutes=task_data.duration_minutes)

    event = CalendarEvent(
        event_id=f"cal_{task_data.task_id}", # Simple way to generate a unique event ID
        summary=event_summary,
        start_time=task_data.start_time,
        end_time=calculated_end_time,
        description=task_data.description,
        task_id_ref=task_data.task_id
    )
    return event

# Placeholder for future calendar API interaction
def add_event_to_calendar_api(event: CalendarEvent) -> Dict[str, Any]:
    """
    Placeholder function to simulate adding an event to an external calendar API.

    Args:
        event: The CalendarEvent to add.

    Returns:
        A dictionary with the API response (simulated).
    """
    print(f"Simulating: Adding event '{event.summary}' to calendar API.")
    # In a real scenario, this would involve HTTP requests, authentication, etc.
    # Example: google_calendar_service.add_event(event.dict())
    return {"status": "success", "event_id_api": f"api_{event.event_id}", "message": "Event hypothetically added."}

# Placeholder for interaction with a ScheduleAgent
def block_time_with_schedule_agent(task_id: str, start_time: datetime, duration_minutes: int) -> Dict[str, Any]:
    """
    Placeholder function to simulate blocking time via a ScheduleAgent.

    Args:
        task_id: The ID of the task for which to block time.
        start_time: The start time for the block.
        duration_minutes: The duration of the block in minutes.

    Returns:
        A dictionary with the ScheduleAgent's response (simulated).
    """
    print(f"Simulating: Requesting ScheduleAgent to block {duration_minutes} min for task {task_id} starting at {start_time}.")
    # Example: schedule_agent.block_time(task_id, start_time, duration_minutes)
    return {"status": "success", "block_id": f"sched_{task_id}", "message": "Time hypothetically blocked by ScheduleAgent."}

if __name__ == '__main__':
    # Example Usage (for testing purposes)
    try:
        sample_task_data_valid = TaskInput(
            task_id="task123",
            description="Plan the quarterly review meeting with the team. Discuss Q1 performance and Q2 goals.",
            summary="Quarterly Review Meeting Prep",
            start_time=datetime.now() + timedelta(days=1),
            duration_minutes=60
        )

        calendar_event = create_calendar_event_from_task(sample_task_data_valid)
        print("\n--- Created Calendar Event ---")
        print(calendar_event.model_dump_json(indent=2))

        api_response = add_event_to_calendar_api(calendar_event)
        print("\n--- API Simulation Response ---")
        print(api_response)

        schedule_agent_response = block_time_with_schedule_agent(
            task_id=sample_task_data_valid.task_id,
            start_time=sample_task_data_valid.start_time,
            duration_minutes=sample_task_data_valid.duration_minutes
        )
        print("\n--- ScheduleAgent Simulation Response ---")
        print(schedule_agent_response)

        # Example of invalid data for Pydantic validation (duration_minutes <= 0)
        print("\n--- Testing Invalid Duration (Pydantic Validation) ---")
        invalid_task_data_duration = TaskInput(
            task_id="task_invalid_duration",
            description="This task has an invalid duration.",
            summary="Invalid Duration Test",
            start_time=datetime.now(),
            duration_minutes=-30 # Invalid duration
        )
        # The above line will raise ValueError due to Pydantic's gt=0 validator

    except TypeError as e:
        print(f"\nError creating TaskInput or CalendarEvent: {e}")
    except ValueError as e: # Catches Pydantic validation errors as well
        print(f"\nValidation Error for TaskInput or CalendarEvent: {e}")
    except Exception as e:
        print(f"\nAn unexpected error occurred: {e}")

    # Example of end_time validation within CalendarEvent
    try:
        print("\n--- Testing Invalid end_time (Custom Validator) ---")
        # Directly creating a CalendarEvent with end_time <= start_time
        invalid_event_times = CalendarEvent(
            event_id="cal_invalid_time",
            summary="Invalid Event Times",
            start_time=datetime.now(),
            end_time=datetime.now() - timedelta(hours=1), # end_time before start_time
            description="Test event with invalid times.",
            task_id_ref="task_ref_invalid_time"
        )
        # The above line will raise ValueError due to the custom validator
    except ValueError as e:
        print(f"\nValidation Error for CalendarEvent: {e}")
    except Exception as e:
        print(f"\nAn unexpected error occurred: {e}")
