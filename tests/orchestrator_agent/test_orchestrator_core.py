import pytest
from orchestrator_agent.orchestrator_core import OrchestrationEngine, AgentResponse

# Mock agent classes as defined in the example usage
class MockScheduleAgent:
    def handle_schedule_request(self, entities):
        # This mock should ideally return something that aligns with AgentResponse structure expectations
        # For the current OrchestrationEngine, it doesn't directly consume this return value's structure,
        # but good practice for future.
        return {"status": "scheduled_by_mock", "details": entities}

class MockTaskAgent:
    def handle_task_request(self, entities):
        return {"status": "task_created_by_mock", "details": entities}

class MockMemoryManager:
    def get_context_for_agent(self, user_id, agent_name, query):
        # This method is called but its return is not used in current routing logic,
        # so a simple print or log might be enough for the mock if we were to check calls to it.
        # print(f"MockMemoryManager.get_context_for_agent called with: {user_id}, {agent_name}, {query}")
        return f"Mock context for {user_id} regarding {query} for {agent_name}"

@pytest.fixture
def mock_schedule_agent_fixture(): # Renamed to avoid conflict with class name if imported directly
    return MockScheduleAgent()

@pytest.fixture
def mock_task_agent_fixture(): # Renamed
    return MockTaskAgent()

@pytest.fixture
def mock_memory_manager_fixture(): # Renamed
    return MockMemoryManager()

@pytest.fixture
def orchestration_engine_fixture(mock_schedule_agent_fixture, mock_task_agent_fixture, mock_memory_manager_fixture):
    available_agents = {
        'schedule_agent': mock_schedule_agent_fixture,
        'task_agent': mock_task_agent_fixture
    }
    engine = OrchestrationEngine(
        memory_manager_agent_client=mock_memory_manager_fixture,
        available_agents_map=available_agents
    )
    return engine

def test_route_request_schedule_intent(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'extract_schedule_info', 'entities': {'title': 'Meeting', 'date': 'Tomorrow'}}
    user_id = 'user123'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['data'] == {'message': 'Successfully routed to schedule_agent', 'entities': {'title': 'Meeting', 'date': 'Tomorrow'}}
    assert response['message'] == 'Schedule request processed by orchestrator.'
    assert response['source_agent'] == 'OrchestratorAgent'

def test_route_request_task_intent(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'create_task', 'entities': {'task_description': 'Buy milk'}}
    user_id = 'user456'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['data'] == {'message': 'Successfully routed to task_agent', 'entities': {'task_description': 'Buy milk'}}
    assert response['message'] == 'Task request processed by orchestrator.'
    assert response['source_agent'] == 'OrchestratorAgent'

def test_route_request_general_conversation(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'general_conversation', 'response_text': 'Hello, how are you?'}
    user_id = 'user789'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['data'] == {'response': 'Hello, how are you?'}
    assert response['message'] == 'General conversation handled by orchestrator.'
    assert response['source_agent'] == 'OrchestratorAgent'

def test_route_request_unknown_intent(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'some_unknown_intent', 'entities': {}}
    user_id = 'user000'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'error'
    assert response['data'] is None
    assert response['message'] == "Unknown intent: some_unknown_intent"
    assert response['source_agent'] == 'OrchestratorAgent'

def test_route_request_missing_agent(mock_memory_manager_fixture): # Uses only memory manager
    # Initialize OrchestrationEngine with an empty available_agents_map
    engine = OrchestrationEngine(
        memory_manager_agent_client=mock_memory_manager_fixture,
        available_agents_map={} # No agents available
    )
    intent_data = {'intent': 'extract_schedule_info', 'entities': {'title': 'Meeting'}}
    user_id = 'user123'
    response = engine.route_request(intent_data, user_id)

    assert response['status'] == 'error'
    assert response['data'] is None
    assert response['message'] == "Agent for intent 'extract_schedule_info' not found."
    assert response['source_agent'] == 'OrchestratorAgent'

    # Also test for task_agent missing
    intent_data_task = {'intent': 'create_task', 'entities': {'task_description': 'Buy milk'}}
    response_task = engine.route_request(intent_data_task, user_id)
    assert response_task['status'] == 'error'
    assert response_task['data'] is None
    assert response_task['message'] == "Agent for intent 'create_task' not found."
    assert response_task['source_agent'] == 'OrchestratorAgent'
