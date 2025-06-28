import pytest

from orchestrator_agent.orchestrator_core import OrchestrationEngine, AgentResponse
from orchestrator_agent.base_agent import BaseAgent
from schedule_agent.schedule_agent import ScheduleAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent

from typing import Dict, Any, List

class SimpleTaskAgent(BaseAgent):
    @property
    def name(self) -> str:
        return "simple_task_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["create_task"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        return AgentResponse(
            status="success",
            data={"task_id": "task123", "user": user_id, "received": entities},
            message="Task created",
            source_agent=self.name,
        )

def test_multi_agent_flow_end_to_end():
    mm = MemoryManagerAgent()
    engine = OrchestrationEngine(memory_manager_client=mm)

    schedule_agent = ScheduleAgent()
    task_agent = SimpleTaskAgent()

    engine.register_agent(schedule_agent)
    engine.register_agent(task_agent)

    user_id = "multi_user"

    # First, route a schedule intent
    schedule_intent = {
        "intent": "extract_schedule_info",
        "entities": {"title": "Demo", "date": "Tomorrow", "time": "10 AM"},
    }
    resp_schedule = engine.route_request(schedule_intent, user_id)
    assert resp_schedule["status"] == "success"
    assert resp_schedule["data"]["agent_response"]["source_agent"] == "schedule_agent"

    # Then, route a task intent
    task_intent = {
        "intent": "create_task",
        "entities": {"task_description": "Prepare slides"},
    }
    resp_task = engine.route_request(task_intent, user_id)
    assert resp_task["status"] == "success"
    assert resp_task["data"]["agent_response"]["source_agent"] == "simple_task_agent"
