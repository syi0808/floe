import pytest
from orchestrator_agent.orchestrator_core import OrchestrationEngine, AgentResponse
from orchestrator_agent.base_agent import BaseAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent # Import MemoryManagerAgent
from conversation_agent.orchestrator_wrapper import ConversationAgentWrapper
from inbox_agent.inbox_agent import InboxAgent
from health_agent.health_agent import HealthAgent
from insight_agent.insight_agent import InsightAgent
from typing import Dict, Any, List

# Refactored Mock agent classes to inherit from BaseAgent
class MockScheduleAgent(BaseAgent):
    @property
    def name(self) -> str:
        return "mock_schedule_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ['extract_schedule_info']

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        return AgentResponse(
            status='success',
            data={'confirmation_id': 'sched_mock_123', 'user': user_id, 'processed_entities': entities},
            message=f'Schedule processed by {self.name}',
            source_agent=self.name
        )

class MockTaskAgent(BaseAgent):
    @property
    def name(self) -> str:
        return "mock_task_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ['create_task']

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        return AgentResponse(
            status='success',
            data={'task_id': 'task_mock_456', 'user': user_id, 'processed_entities': entities},
            message=f'Task processed by {self.name}',
            source_agent=self.name
        )

# MockMemoryManager class is removed, using the actual MemoryManagerAgent

@pytest.fixture
def mock_schedule_agent_fixture():
    return MockScheduleAgent()

@pytest.fixture
def mock_task_agent_fixture():
    return MockTaskAgent()

@pytest.fixture
def inbox_agent_fixture():
    return InboxAgent()

@pytest.fixture
def health_agent_fixture():
    return HealthAgent()

@pytest.fixture
def insight_agent_fixture():
    return InsightAgent()

@pytest.fixture
def mock_memory_manager_fixture(): # Keep name for consistency, but it's the real one
    return MemoryManagerAgent()

@pytest.fixture
def orchestration_engine_fixture(mock_schedule_agent_fixture: MockScheduleAgent, mock_task_agent_fixture: MockTaskAgent, mock_memory_manager_fixture: MemoryManagerAgent): # Updated type hint
        engine = OrchestrationEngine(memory_manager_client=mock_memory_manager_fixture) # Updated parameter name
        # Register agents using the new signature
        engine.register_agent(mock_schedule_agent_fixture)
        engine.register_agent(mock_task_agent_fixture)
        return engine

@pytest.fixture
def orchestration_engine_with_conv_fixture(mock_schedule_agent_fixture: MockScheduleAgent, mock_task_agent_fixture: MockTaskAgent, mock_memory_manager_fixture: MemoryManagerAgent):
    engine = OrchestrationEngine(memory_manager_client=mock_memory_manager_fixture)
    engine.register_agent(mock_schedule_agent_fixture)
    engine.register_agent(mock_task_agent_fixture)
    engine.register_agent(ConversationAgentWrapper())
    return engine

@pytest.fixture
def orchestration_engine_with_all_agents(
    mock_schedule_agent_fixture: MockScheduleAgent,
    mock_task_agent_fixture: MockTaskAgent,
    inbox_agent_fixture: InboxAgent,
    health_agent_fixture: HealthAgent,
    insight_agent_fixture: InsightAgent,
    mock_memory_manager_fixture: MemoryManagerAgent,
):
    engine = OrchestrationEngine(memory_manager_client=mock_memory_manager_fixture)
    engine.register_agent(mock_schedule_agent_fixture)
    engine.register_agent(mock_task_agent_fixture)
    engine.register_agent(inbox_agent_fixture)
    engine.register_agent(health_agent_fixture)
    engine.register_agent(insight_agent_fixture)
    return engine

def test_route_request_schedule_intent(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'extract_schedule_info', 'entities': {'title': 'Meeting', 'date': 'Tomorrow'}}
    user_id = 'user123'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['source_agent'] == 'OrchestratorAgent' # Orchestrator is the source of this top-level response

    # Check orchestrator's message and passed-through entities
    assert response['data']['message'] == "Successfully routed to mock_schedule_agent for intent extract_schedule_info"
    assert response['data']['entities'] == intent_data['entities']
    assert response['message'] == 'extract_schedule_info request processed by orchestrator via mock_schedule_agent.'

    # Check the agent_response data from the mock agent
    assert 'agent_response' in response['data']
    agent_resp = response['data']['agent_response']

    assert agent_resp['status'] == 'success'
    assert agent_resp['source_agent'] == 'mock_schedule_agent'
    assert 'confirmation_id' in agent_resp['data']
    assert agent_resp['data']['user'] == user_id
    assert agent_resp['data']['processed_entities'] == intent_data['entities']
    assert agent_resp['message'] == 'Schedule processed by mock_schedule_agent'

def test_route_request_task_intent(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'create_task', 'entities': {'task_description': 'Buy milk'}}
    user_id = 'user456'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['source_agent'] == 'OrchestratorAgent'

    # Check orchestrator's message and passed-through entities
    assert response['data']['message'] == "Successfully routed to mock_task_agent for intent create_task"
    assert response['data']['entities'] == intent_data['entities']
    assert response['message'] == 'create_task request processed by orchestrator via mock_task_agent.'

    # Check the agent_response data from the mock agent
    assert 'agent_response' in response['data']
    agent_resp = response['data']['agent_response']

    assert agent_resp['status'] == 'success'
    assert agent_resp['source_agent'] == 'mock_task_agent'
    assert 'task_id' in agent_resp['data']
    assert agent_resp['data']['user'] == user_id
    assert agent_resp['data']['processed_entities'] == intent_data['entities']
    assert agent_resp['message'] == 'Task processed by mock_task_agent'

def test_route_request_general_conversation(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'general_conversation', 'response_text': 'Hello, how are you?'}
    user_id = 'user789'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['data'] == {'response': 'Hello, how are you?'}
    assert response['message'] == 'General conversation handled by orchestrator.'
    assert response['source_agent'] == 'OrchestratorAgent'

def test_general_conversation_routed_to_agent(orchestration_engine_with_conv_fixture: OrchestrationEngine):
    intent_data = {'intent': 'general_conversation', 'response_text': 'Hi there'}
    user_id = 'conv_user'
    response = orchestration_engine_with_conv_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert 'agent_response' in response['data']
    agent_resp = response['data']['agent_response']
    assert agent_resp['source_agent'] == 'conversation_agent'
    assert agent_resp['data']['response'] == 'Hello! How can I assist you today?'

def test_route_request_unknown_intent(orchestration_engine_fixture: OrchestrationEngine):
    intent_data = {'intent': 'some_unknown_intent', 'entities': {}}
    user_id = 'user000'
    response = orchestration_engine_fixture.route_request(intent_data, user_id)

    assert response['status'] == 'error'
    assert response['data'] is None
    assert response['message'] == "Unknown intent or no agent available for intent: some_unknown_intent"
    assert response['source_agent'] == 'OrchestratorAgent'

def test_route_request_missing_agent(mock_memory_manager_fixture: MemoryManagerAgent): # Updated type hint
    # Initialize OrchestrationEngine without registering any agents for specific intents
    engine = OrchestrationEngine(memory_manager_client=mock_memory_manager_fixture) # Updated parameter name

    intent_data = {'intent': 'extract_schedule_info', 'entities': {'title': 'Meeting'}}
    user_id = 'user123'
    response = engine.route_request(intent_data, user_id)

    assert response['status'] == 'error'
    assert response['data'] is None
    assert response['message'] == "Unknown intent or no agent available for intent: extract_schedule_info"
    assert response['source_agent'] == 'OrchestratorAgent'

    # Also test for task_agent missing
    intent_data_task = {'intent': 'create_task', 'entities': {'task_description': 'Buy milk'}}
    response_task = engine.route_request(intent_data_task, user_id)
    assert response_task['status'] == 'error'
    assert response_task['data'] is None
    assert response_task['message'] == "Unknown intent or no agent available for intent: create_task"
    assert response_task['source_agent'] == 'OrchestratorAgent'


def test_inbox_agent_routing(orchestration_engine_with_all_agents: OrchestrationEngine):
    intent_data = {'intent': 'process_email', 'entities': {'subject': 'Hello'}}
    user_id = 'inbox_user'
    response = orchestration_engine_with_all_agents.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['data']['message'] == 'Successfully routed to inbox_agent for intent process_email'
    agent_resp = response['data']['agent_response']
    assert agent_resp['source_agent'] == 'inbox_agent'
    assert agent_resp['data']['received']['subject'] == 'Hello'


def test_health_agent_routing(orchestration_engine_with_all_agents: OrchestrationEngine):
    intent_data = {'intent': 'log_health_data', 'entities': {'steps': 1000}}
    user_id = 'health_user'
    response = orchestration_engine_with_all_agents.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['data']['message'] == 'Successfully routed to health_agent for intent log_health_data'
    agent_resp = response['data']['agent_response']
    assert agent_resp['source_agent'] == 'health_agent'
    assert agent_resp['data']['received']['steps'] == 1000


def test_insight_agent_routing(orchestration_engine_with_all_agents: OrchestrationEngine):
    intent_data = {'intent': 'generate_insight_report', 'entities': {'period': '2025-W20'}}
    user_id = 'insight_user'
    response = orchestration_engine_with_all_agents.route_request(intent_data, user_id)

    assert response['status'] == 'success'
    assert response['data']['message'] == 'Successfully routed to insight_agent for intent generate_insight_report'
    agent_resp = response['data']['agent_response']
    assert agent_resp['source_agent'] == 'insight_agent'
    assert agent_resp['data']['received']['period'] == '2025-W20'
