import pytest
from datetime import datetime, timedelta
from pydantic import ValidationError

from task_agent.task_calendar_linker import (
    TaskInput,
    CalendarEvent,
    create_calendar_event_from_task,
    add_event_to_calendar_api,
    block_time_with_schedule_agent
)

# Sample valid task data for reuse
SAMPLE_START_TIME = datetime.now() + timedelta(days=1)
VALID_TASK_DATA = {
    "task_id": "task_001",
    "description": "This is a detailed description of the task for planning.",
    "start_time": SAMPLE_START_TIME,
    "duration_minutes": 60,
    "summary": "Task Summary"
}

class TestTaskInput:
    def test_valid_task_input(self):
        task_input = TaskInput(**VALID_TASK_DATA)
        assert task_input.task_id == VALID_TASK_DATA["task_id"]
        assert task_input.description == VALID_TASK_DATA["description"]
        assert task_input.start_time == VALID_TASK_DATA["start_time"]
        assert task_input.duration_minutes == VALID_TASK_DATA["duration_minutes"]
        assert task_input.summary == VALID_TASK_DATA["summary"]

    def test_task_input_missing_required_fields(self):
        with pytest.raises(ValidationError, match=r".*task_id\s*Field required.*"):
            TaskInput(description="Only description provided", start_time=SAMPLE_START_TIME, duration_minutes=30)

        with pytest.raises(ValidationError, match=r".*description\s*Field required.*"):
            TaskInput(task_id="t1", start_time=SAMPLE_START_TIME, duration_minutes=30)

        with pytest.raises(ValidationError, match=r".*start_time\s*Field required.*"):
            TaskInput(task_id="t1", description="desc", duration_minutes=30)

        with pytest.raises(ValidationError, match=r".*duration_minutes\s*Field required.*"):
            TaskInput(task_id="t1", description="desc", start_time=SAMPLE_START_TIME)


    def test_task_input_invalid_duration(self):
        with pytest.raises(ValidationError, match=r".*duration_minutes\s*Input should be greater than 0.*"):
            TaskInput(**{**VALID_TASK_DATA, "duration_minutes": 0})

        with pytest.raises(ValidationError, match=r".*duration_minutes\s*Input should be greater than 0.*"):
            TaskInput(**{**VALID_TASK_DATA, "duration_minutes": -30})

    def test_task_input_invalid_types(self):
        with pytest.raises(ValidationError, match=r".*duration_minutes\s*Input should be a valid integer.*"):
            TaskInput(**{**VALID_TASK_DATA, "duration_minutes": "60 minutes"})

        with pytest.raises(ValidationError, match=r".*start_time\s*Input should be a valid datetime.*"):
            TaskInput(**{**VALID_TASK_DATA, "start_time": "tomorrow"})

    def test_task_input_summary_optional(self):
        task_data_no_summary = VALID_TASK_DATA.copy()
        del task_data_no_summary["summary"]
        task_input = TaskInput(**task_data_no_summary)
        assert task_input.summary is None


class TestCalendarEvent:
    def test_valid_calendar_event(self):
        start = datetime.now()
        end = start + timedelta(hours=1)
        event = CalendarEvent(
            event_id="evt_001",
            summary="Valid Event",
            start_time=start,
            end_time=end,
            task_id_ref="task_ref_001"
        )
        assert event.start_time == start
        assert event.end_time == end

    def test_calendar_event_end_time_before_start_time(self):
        start = datetime.now()
        end_before_start = start - timedelta(hours=1)
        with pytest.raises(ValidationError, match="End time must be after start time"):
            CalendarEvent(
                event_id="evt_002",
                summary="Invalid Event Times",
                start_time=start,
                end_time=end_before_start,
                task_id_ref="task_ref_002"
            )

    def test_calendar_event_end_time_equals_start_time(self):
        start = datetime.now()
        with pytest.raises(ValidationError, match="End time must be after start time"):
            CalendarEvent(
                event_id="evt_003",
                summary="Invalid Event Times",
                start_time=start,
                end_time=start, # Same as start_time
                task_id_ref="task_ref_003"
            )


class TestCreateCalendarEventFromTask:
    def test_create_event_with_valid_task_input(self):
        task_input = TaskInput(**VALID_TASK_DATA)
        calendar_event = create_calendar_event_from_task(task_input)

        assert calendar_event.event_id == f"cal_{VALID_TASK_DATA['task_id']}"
        assert calendar_event.summary == VALID_TASK_DATA["summary"]
        assert calendar_event.start_time == VALID_TASK_DATA["start_time"]
        expected_end_time = VALID_TASK_DATA["start_time"] + timedelta(minutes=VALID_TASK_DATA["duration_minutes"])
        assert calendar_event.end_time == expected_end_time
        assert calendar_event.description == VALID_TASK_DATA["description"]
        assert calendar_event.task_id_ref == VALID_TASK_DATA["task_id"]

    def test_create_event_task_summary_is_none(self):
        task_data_no_summary = VALID_TASK_DATA.copy()
        del task_data_no_summary["summary"]
        task_input = TaskInput(**task_data_no_summary)
        calendar_event = create_calendar_event_from_task(task_input)

        expected_summary = task_input.description[:100]
        assert calendar_event.summary == expected_summary

    def test_create_event_with_long_description_for_summary(self):
        long_desc = "a" * 150
        task_data_long_desc = {
            **VALID_TASK_DATA,
            "description": long_desc,
        }
        del task_data_long_desc["summary"] # Ensure summary is derived
        task_input = TaskInput(**task_data_long_desc)
        calendar_event = create_calendar_event_from_task(task_input)
        assert calendar_event.summary == long_desc[:100]
        assert len(calendar_event.summary) == 100


    def test_create_event_type_error(self):
        with pytest.raises(TypeError, match="task_data must be an instance of TaskInput"):
            create_calendar_event_from_task({"not_a_task_input_object": True})


class TestPlaceholderFunctions:
    def test_add_event_to_calendar_api(self, capsys):
        start = datetime.now()
        event = CalendarEvent(
            event_id="evt_placeholder_01",
            summary="Test API Event",
            start_time=start,
            end_time=start + timedelta(hours=1),
            task_id_ref="task_api_test"
        )
        response = add_event_to_calendar_api(event)

        captured = capsys.readouterr()
        assert f"Simulating: Adding event '{event.summary}' to calendar API." in captured.out
        assert response["status"] == "success"
        assert response["event_id_api"] == f"api_{event.event_id}"
        assert "Event hypothetically added" in response["message"]

    def test_block_time_with_schedule_agent(self, capsys):
        task_id = "task_sched_test_01"
        start_time = datetime.now() + timedelta(days=2)
        duration_minutes = 90

        response = block_time_with_schedule_agent(task_id, start_time, duration_minutes)

        captured = capsys.readouterr()
        assert f"Simulating: Requesting ScheduleAgent to block {duration_minutes} min for task {task_id} starting at {start_time}." in captured.out
        assert response["status"] == "success"
        assert response["block_id"] == f"sched_{task_id}"
        assert "Time hypothetically blocked by ScheduleAgent" in response["message"]

# Example of how to run with pytest from the root directory:
# PYTHONPATH=. pytest tests/task_agent/test_task_calendar_linker.py
