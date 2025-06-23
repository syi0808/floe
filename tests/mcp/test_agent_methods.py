import pytest
from unittest.mock import MagicMock

from schedule_agent.schedule_agent import ScheduleAgent
from task_agent.task_agent import TaskAgent
from inbox_agent.inbox_agent import InboxAgent
from health_agent.health_agent import HealthAgent
from insight_agent.insight_agent import InsightAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent


@pytest.mark.parametrize(
    "agent_cls,method_name",
    [
        (ScheduleAgent, "add_memory"),
        (TaskAgent, "add_memory"),
        (InboxAgent, "add_memory"),
        (HealthAgent, "add_memory"),
        (InsightAgent, "add_memory"),
        (MemoryManagerAgent, "post_memory_to_mcp"),
    ],
)
def test_agents_delegate_add_memory(agent_cls, method_name):
    mock_client = MagicMock()
    agent = agent_cls(mcp_client=mock_client)
    getattr(agent, method_name)("u1", {"type": "t", "content": "c"})
    mock_client.add_memory.assert_called_once_with("u1", {"type": "t", "content": "c"})
