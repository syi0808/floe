from orchestrator_agent.base_agent import BaseAgent
from orchestrator_agent.common_types import AgentResponse # Updated import
from typing import Dict, Any, List

class ScheduleAgent(BaseAgent):
    def __init__(self):
        super().__init__() # Ensure BaseAgent's __init__ is called if it has one
        print("ScheduleAgent initialized.")

    @property
    def name(self) -> str:
        return "schedule_agent"

    @property
    def supported_intents(self) -> List[str]:
        # This should align with the intent name produced by IntentAnalyzer
        # which uses ExtractScheduleInfoTool.name
        return ["extract_schedule_info"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        print(f"--- {self.name} processing ---")
        print(f"User ID: {user_id}")
        print(f"Received entities: {entities}")

        # Basic validation of expected entities from ExtractScheduleInfoTool
        title = entities.get("title")
        participants = entities.get("participants")
        time = entities.get("time")
        date = entities.get("date")
        # description is optional

        if not all([title, participants, time, date]):
            return AgentResponse(
                status='error',
                data={'missing_fields': True, 'received_entities': entities},
                message=f"{self.name}: Missing required entities (title, participants, time, date) for scheduling.",
                source_agent=self.name
            )

        # In a real implementation, this is where you would:
        # 1. Parse date/time expressions more robustly (e.g., using dateparser).
        # 2. Convert to UTC datetime objects.
        # 3. Interact with calendar APIs (Google, Microsoft) via connectors.
        # 4. Check for conflicts.
        # 5. Create the event.
        # For now, we'll just confirm processing.

        confirmation_message = f"Event '{title}' with {', '.join(participants)} on {date} at {time} notionally processed."
        event_id_mock = f"evt_mock_{hash(str(entities))%10000}"

        print(f"{self.name}: {confirmation_message}")

        return AgentResponse(
            status='success',
            data={
                'event_id': event_id_mock,
                'details': entities,
                'user_id': user_id,
                'confirmation_message': confirmation_message
            },
            message=f"Successfully processed schedule request by {self.name}.",
            source_agent=self.name
        )

# Example of direct invocation for testing (optional)
# if __name__ == '__main__':
#     agent = ScheduleAgent()
#     mock_entities = {
#         "title": "Team Sync",
#         "participants": ["Alice", "Bob"],
#         "time": "2 PM",
#         "date": "Next Monday",
#         "description": "Discuss Q3 goals"
#     }
#     mock_user_id = "user_test_123"
#     response = agent.process(mock_entities, mock_user_id)
#     print("\n--- Direct Test Response ---")
#     print(response)

#     mock_entities_fail = {
#         "title": "Missing Time",
#         "participants": ["Charlie"],
#         # "time": "3 PM", # Missing
#         "date": "Tomorrow"
#     }
#     response_fail = agent.process(mock_entities_fail, mock_user_id)
#     print("\n--- Direct Test Response (Failure Case) ---")
#     print(response_fail)
