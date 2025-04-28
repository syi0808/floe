import datetime
from typing import Any, AsyncIterable, Dict, List
from google.adk.agents.llm_agent import LlmAgent
from google.adk.artifacts import InMemoryArtifactService
from google.adk.memory.in_memory_memory_service import InMemoryMemoryService
from google.adk.runners import Runner
from google.adk.sessions import InMemorySessionService
from google.genai import types
from common.agent.base_agent import BaseAgent
from common.llm.global_model_llm import GlobalModelLlm

# Helper functions for ScheduleAgent tools


def parse_schedule_text(request: str) -> Dict[str, Any]:
    """
    Parses a natural language schedule request into structured event data.
    Expects a string like '다음 주 화요일 오후 3시에 회의 잡아줘'.
    Returns a JSON dict with keys: start, end, title, participants.
    """
    # TODO: 실제 NLP 파싱 로직 연동
    # 예시 반환 포맷
    return {
        "start": "2025-05-06T15:00:00+09:00",
        "end": "2025-05-06T16:00:00+09:00",
        "title": "회의",
        "participants": ["user@example.com"],
    }


# 예시: in-memory calendar
_calendar: List[Dict[str, Any]] = []


def check_conflict(event: Dict[str, Any]) -> Dict[str, Any]:
    """
    Checks for time conflicts against an in-memory calendar.
    """
    for existing in _calendar:
        if not (event["end"] <= existing["start"] or event["start"] >= existing["end"]):
            return {"conflict": True, "with": existing}
    return {"conflict": False}


def suggest_time_slot(duration_minutes: int = 60) -> Dict[str, Any]:
    """
    Suggests the next available time slot of given duration.
    """
    now = datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=9)))
    start = now.replace(minute=0, second=0, microsecond=0) + datetime.timedelta(hours=1)
    end = start + datetime.timedelta(minutes=duration_minutes)
    return {"suggested_start": start.isoformat(), "suggested_end": end.isoformat()}


def create_event(event: Dict[str, Any]) -> Dict[str, Any]:
    """
    Adds the event to the in-memory calendar and returns confirmation.
    """
    _calendar.append(event)
    return {"status": "created", "event": event}


def summarize_schedule() -> Dict[str, Any]:
    """
    Returns a summary of all upcoming events.
    """
    return {"events": _calendar}


class ScheduleAgent(BaseAgent):
    """An agent that handles scheduling tasks."""

    SUPPORTED_CONTENT_TYPES = ["text", "text/plain", "application/json"]

    def __init__(self):
        self._agent = self._build_agent()
        self._user_id = "schedule_agent"
        self._runner = Runner(
            app_name=self._agent.name,
            agent=self._agent,
            artifact_service=InMemoryArtifactService(),
            session_service=InMemorySessionService(),
            memory_service=InMemoryMemoryService(),
        )

    def invoke(self, query: str, session_id: str) -> str:
        session = self._runner.session_service.get_session(
            app_name=self._agent.name, user_id=self._user_id, session_id=session_id
        )
        content = types.Content(role="user", parts=[types.Part.from_text(text=query)])
        if session is None:
            session = self._runner.session_service.create_session(
                app_name=self._agent.name,
                user_id=self._user_id,
                state={},
                session_id=session_id,
            )
        events = list(
            self._runner.run(
                user_id=self._user_id, session_id=session.id, new_message=content
            )
        )
        if not events or not events[-1].content or not events[-1].content.parts:
            return ""
        return "\n".join([p.text for p in events[-1].content.parts if p.text])

    async def stream(
        self, query: str, session_id: str
    ) -> AsyncIterable[Dict[str, Any]]:
        session = self._runner.session_service.get_session(
            app_name=self._agent.name, user_id=self._user_id, session_id=session_id
        )
        content = types.Content(role="user", parts=[types.Part.from_text(text=query)])
        if session is None:
            session = self._runner.session_service.create_session(
                app_name=self._agent.name,
                user_id=self._user_id,
                state={},
                session_id=session_id,
            )
        async for event in self._runner.run_async(
            user_id=self._user_id, session_id=session.id, new_message=content
        ):
            if event.is_final_response():
                response = ""
                if event.content and event.content.parts:
                    response = "\n".join(
                        [p.text for p in event.content.parts if p.text]
                    )
                yield {"is_task_complete": True, "content": response}
            else:
                yield {
                    "is_task_complete": False,
                    "updates": "Processing schedule request...",
                }

    def _build_agent(self) -> LlmAgent:
        """Builds the LLM agent for schedule handling."""
        return LlmAgent(
            model=GlobalModelLlm,
            name="schedule_agent",
            description="일정 생성, 충돌 감지, 시간 추천, 일정 요약 등을 처리하는 에이전트입니다.",
            instruction="""
You are ScheduleAgent.
Handle user scheduling requests by using the provided tools:
- parse_schedule_text: parse natural language into event structure
- check_conflict: detect conflicts
- suggest_time_slot: suggest next free slot
- create_event: add event to calendar
- summarize_schedule: list upcoming events
Respond in JSON when using tool outputs, and plain text for final messages.
            """,
            tools=[
                parse_schedule_text,
                check_conflict,
                suggest_time_slot,
                create_event,
                summarize_schedule,
            ],
        )
