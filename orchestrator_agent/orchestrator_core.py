from typing import TypedDict, Optional, Any, Dict, List
from .base_agent import BaseAgent # Import BaseAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent # Import MemoryManagerAgent

class AgentResponse(TypedDict):
    status: str  # e.g., 'success', 'error'
    data: Optional[Any]
    message: Optional[str]
    source_agent: str

class OrchestrationEngine:
    def __init__(self, memory_manager_client: Optional[MemoryManagerAgent] = None): # Updated type hint
        if memory_manager_client is None:
            self.memory_manager = MemoryManagerAgent() # Create a default instance
        else:
            self.memory_manager = memory_manager_client
        # Changed type hint for agents_map to use BaseAgent
        self.agents_map: Dict[str, BaseAgent] = {}
        self.intent_to_agent_map: Dict[str, str] = {}

    # Modified register_agent to use BaseAgent and its supported_intents property
    def register_agent(self, agent_instance: BaseAgent):
        # agent_name is now retrieved from the agent instance itself
        agent_name = agent_instance.name
        self.agents_map[agent_name] = agent_instance
        for intent in agent_instance.supported_intents:
            self.intent_to_agent_map[intent] = agent_name

    def route_request(self, intent_data: Dict[str, Any], user_id: str) -> AgentResponse:
        intent = intent_data.get('intent')
        agent_name = self.intent_to_agent_map.get(intent) # Determine agent_name early

        # Actual MemoryManager Interaction
        if self.memory_manager: # Check if memory_manager is available
            current_query_summary = str(intent_data.get('entities', intent_data.get('response_text', '')))
            # Determine agent_name for context; use "OrchestratorDirect" if no specific agent for intent
            context_agent_name = agent_name if agent_name else "OrchestratorDirect"
            context = self.memory_manager.get_context_for_agent(
                user_id=user_id,
                agent_name=context_agent_name,
                query_text=current_query_summary
            )
            print(f"OrchestrationEngine: Retrieved context for user '{user_id}', agent '{context_agent_name}': {context}")
            # TODO: Pass context to agent.process() method or use it in routing decisions

        intent = intent_data.get('intent')
        entities = intent_data.get('entities')

        # Handle general_conversation first
        if intent == 'general_conversation':
            return AgentResponse(
                status='success',
                data={'response': intent_data.get('response_text')},
                message='General conversation handled by orchestrator.',
                source_agent='OrchestratorAgent'
            )

        # Dynamic Agent Dispatch
        agent_name = self.intent_to_agent_map.get(intent)

        if agent_name:
            agent_instance = self.agents_map.get(agent_name)
            if agent_instance:
                # Assume the agent instance has a method like process(entities, user_id)
                # This is a MOCK CALL simulation
                try:
                    # mock_agent_response_data = agent_instance.process(
                    #     entities=entities,
                    #     user_id=user_id
                    # )
                    # For now, we'll just indicate success and pass entities.
                    # In a real implementation, agent_instance.process would be called.
                    # If the mock agents provided in the example are used, they do have 'process'.
                    # To make this runnable with current mocks if __name__ == "__main__": is uncommented,
                    # we can actually call process if it exists.
                    agent_specific_response = {}
                    if hasattr(agent_instance, 'process') and callable(getattr(agent_instance, 'process')):
                        agent_specific_response = agent_instance.process(entities=entities, user_id=user_id)

                    return AgentResponse(
                        status='success',
                        data={
                            'message': f'Successfully routed to {agent_name} for intent {intent}',
                            'entities': entities,
                            'agent_response': agent_specific_response # Contains data from mock agent's process method
                        },
                        message=f'{intent} request processed by orchestrator via {agent_name}.',
                        source_agent='OrchestratorAgent'
                    )
                except Exception as e:
                    return AgentResponse(
                        status='error',
                        data={'original_intent': intent, 'error_calling_agent': str(e)},
                        message=f"Error calling agent '{agent_name}' for intent '{intent}'.",
                        source_agent='OrchestratorAgent'
                    )
            else:
                # This case implies an internal inconsistency
                return AgentResponse(
                    status='error',
                    data=None,
                    message=f"Internal configuration error: Agent instance not found for agent name '{agent_name}' mapped to intent '{intent}'.",
                    source_agent='OrchestratorAgent'
                )
        else:
            # No agent is registered for this specific intent
            return AgentResponse(
                status='error',
                data=None,
                message=f"Unknown intent or no agent available for intent: {intent}",
                source_agent='OrchestratorAgent'
            )

# if __name__ == '__main__':
#     # Using the actual MemoryManagerAgent and updated Mock Agents
#     # MockMemoryManager class is removed from here, using the actual one.

#     # Updated Mock Agents to inherit from BaseAgent and implement its abstract properties
#     class MockScheduleAgent(BaseAgent): # This class should be defined or imported if used. For this example, let's assume it's defined.
#         @property
#         def name(self) -> str:
#             return "schedule_agent_v2"

#         @property
#         def supported_intents(self) -> List[str]:
#             return ['extract_schedule_info', 'update_schedule_info']

#         def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
#             print(f"MockScheduleAgent.process called with entities: {entities}, user_id: {user_id}")
#             # Example add_memory call after processing
#             # self.memory_manager.add_memory(user_id, {"type": "action_taken", "agent": self.name, "details": entities})
#             return AgentResponse(
#                 status='success',
#                 data={"status": "scheduled_by_mock_v2", "details": entities, "user": user_id, "confirmation_id": "sched_abc123"},
#                 message='Scheduled by mock.',
#                 source_agent=self.name
#             )

#     class MockTaskAgent(BaseAgent): # This class should be defined or imported.
#         @property
#         def name(self) -> str:
#             return "task_agent_v2"

#         @property
#         def supported_intents(self) -> List[str]:
#             return ['create_task']

#         def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
#             print(f"MockTaskAgent.process called with entities: {entities}, user_id: {user_id}")
#             return AgentResponse(
#                 status='success',
#                 data={"status": "task_created_by_mock_v2", "details": entities, "user": user_id, "task_id": "task_xyz789"},
#                 message='Task created by mock.',
#                 source_agent=self.name
#             )

#     # Use the actual MemoryManagerAgent
#     memory_manager_client = MemoryManagerAgent()
#     # Example: Add a persistent memory item for a user to test retrieval
#     memory_manager_client.add_memory("user123", {"type": "user_preference", "content": "Prefers morning meetings."})

#     engine = OrchestrationEngine(memory_manager_client=memory_manager_client) # Pass the actual MemoryManagerAgent

#     schedule_agent_mock = MockScheduleAgent() # These mocks would need access to memory_manager if they use it
#     task_agent_mock = MockTaskAgent()
#     # If agents need memory_manager, it should be passed to their __init__
#     # e.g., schedule_agent_mock = MockScheduleAgent(memory_manager=memory_manager_client)

#     # Updated register_agent calls
#     engine.register_agent(agent_instance=schedule_agent_mock)
#     engine.register_agent(agent_instance=task_agent_mock)

#     print("\n--- Testing OrchestrationEngine with Registered Agents ---")

#     # Test case 1: Schedule intent (handled by schedule_agent_v2)
#     print("\nTest Case 1: Schedule Intent")
#     schedule_intent = {
#         'intent': 'extract_schedule_info',
#         'entities': {'title': 'Team Meeting', 'date': 'Tomorrow', 'time': '10 AM'}
#     }
#     response = engine.route_request(schedule_intent, 'user123')
#     print(f"Response for schedule_intent: {response}")

#     # Test case 2: Another schedule intent (handled by schedule_agent_v2)
#     print("\nTest Case 2: Update Schedule Intent")
#     update_schedule_intent = {
#         'intent': 'update_schedule_info',
#         'entities': {'event_id': 'evt_123', 'new_time': '3 PM'}
#     }
#     response = engine.route_request(update_schedule_intent, 'user123')
#     print(f"Response for update_schedule_intent: {response}")

#     # Test case 3: Task intent (handled by task_agent_v2)
#     print("\nTest Case 3: Create Task Intent")
#     task_intent = {
#         'intent': 'create_task',
#         'entities': {'task_description': 'Buy groceries', 'due_date': 'Today'}
#     }
#     response = engine.route_request(task_intent, 'user456')
#     print(f"Response for task_intent: {response}")

#     # Test case 4: General conversation
#     print("\nTest Case 4: General Conversation Intent")
#     general_intent = {
#         'intent': 'general_conversation',
#         'response_text': "Hello, how are you?"
#     }
#     response = engine.route_request(general_intent, 'user789')
#     print(f"Response for general_intent: {response}")

#     # Test case 5: Unknown intent (not registered)
#     print("\nTest Case 5: Unknown Intent")
#     unknown_intent = {'intent': 'unknown_action', 'entities': {}}
#     response = engine.route_request(unknown_intent, 'user000')
#     print(f"Response for unknown_intent: {response}")

#     # Test case 6: Intent that should have an agent but instance is somehow missing (for testing internal error)
#     # This requires manually manipulating the maps for a pure unit test of this path.
#     print("\nTest Case 6: Intent with registered agent name but missing instance")
#     engine.intent_to_agent_map['rogue_intent_missing_instance'] = 'non_existent_agent_instance'
#     # We do not add 'non_existent_agent_instance' to engine.agents_map
#     rogue_intent_data = {'intent': 'rogue_intent_missing_instance', 'entities': {}}
#     response = engine.route_request(rogue_intent_data, 'userX')
#     print(f"Response for rogue_intent_missing_instance: {response}")

#     # Test case 7: Agent that fails during processing
#     print("\nTest Case 7: Agent raises exception during process")
#     class FailingAgent(BaseAgent):
#         @property
#         def name(self) -> str:
#             return "failing_agent"

#         @property
#         def supported_intents(self) -> List[str]:
#             return ['failing_intent']

#         def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
#             raise ValueError("Simulated processing error in agent")

#     failing_agent_mock = FailingAgent() # This class should be defined or imported.
#     engine.register_agent(agent_instance=failing_agent_mock)

#     failing_intent_data = {'intent': 'failing_intent', 'entities': {'data': 'some_data'}}
#     response = engine.route_request(failing_intent_data, 'user_fail')
#     print(f"Response for failing_intent: {response}")
