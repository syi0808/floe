from typing import Dict, Any, List
import datetime
from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse
from mcp import MCPClient

from .sleep_module import SleepModule
from .nutrition_module import NutritionModule
from .activity_module import ActivityModule
from .wellness_module import WellnessModule
from .health_models import ActivityRecord, WellnessLog
from .health_predictor import predict_sleep_times, predict_meal_times
from .overwork_analyzer import check_overwork
from .health_reporter import HealthReporter

class HealthAgent(BaseAgent):
    """Stub agent for health data processing."""

    def __init__(self, mcp_client: MCPClient | None = None) -> None:
        super().__init__()
        self.mcp_client = mcp_client or MCPClient.from_env()
        self.sleep_module = SleepModule()
        self.nutrition_module = NutritionModule()
        self.activity_module = ActivityModule()
        self.wellness_module = WellnessModule()
        self.reporter = HealthReporter(
            self.sleep_module,
            self.nutrition_module,
            self.activity_module,
            self.wellness_module,
        )

    @property
    def name(self) -> str:
        return "health_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["log_health_data", "weekly_health_summary"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        if entities.get("weekly_summary"):
            summary = self.reporter.generate_weekly_summary(user_id)
            return AgentResponse(
                status="success",
                data={"summary": summary},
                message="Weekly summary generated.",
                source_agent=self.name,
            )

        if "steps" in entities:
    if "steps" in entities:
        record = ActivityRecord(
            user_id=user_id,
            start_time_utc=datetime.datetime.utcnow(),
            duration_minutes=1.0,  # Representing a point-in-time measurement
            activity_type="steps",
            calories_burned=float(entities["steps"]),  # Store steps count here temporarily
        )
            self.activity_module.log_activity(user_id, record)

        if "stress_level" in entities:
            log = WellnessLog(
                user_id=user_id,
                timestamp_utc=datetime.datetime.utcnow(),
                stress_level=int(entities["stress_level"]),
            )
            self.wellness_module.log_wellness_checkin(user_id, log)

        return AgentResponse(
            status="success",
            data={"received": entities, "user": user_id},
            message="Health data logged.",
            source_agent=self.name,
        )

    # MCP helper methods -------------------------------------------------
    def invoke_service(self, service_name: str, payload: Dict[str, Any]):
        return self.mcp_client.invoke_service(service_name, payload)

    def add_memory(self, user_id: str, memory_item: Dict[str, Any]):
        return self.mcp_client.add_memory(user_id, memory_item)

    def search_memories(self, user_id: str, query: str, top_k: int = 5):
        return self.mcp_client.search_memories(user_id, query, top_k)

    def send_reply(
        self,
        user_id: str,
        session_id: str,
        channel_type: str,
        content: str,
        target_details: Dict[str, Any] | None = None,
    ):
        return self.mcp_client.send_reply(
            user_id,
            session_id,
            channel_type,
            content,
            target_details,
        )

    def send_notification(self, notification: Dict[str, Any]):
        return self.mcp_client.send_notification(notification)
