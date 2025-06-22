import pytest

from memory_manager_agent.memory_manager import MemoryManagerAgent


@pytest.fixture
def agent():
    """Return a fresh MemoryManagerAgent for each test."""
    return MemoryManagerAgent()


@pytest.fixture
def populated_agent(agent):
    """Agent with three memories added for user 'u1'."""
    user_id = "u1"
    agent.add_memory(user_id, {"type": "note", "content": "first"})
    agent.add_memory(user_id, {"type": "note", "content": "second"})
    agent.add_memory(user_id, {"type": "note", "content": "third"})
    return agent, user_id


def test_get_context_returns_last_memories_in_order(populated_agent):
    agent, user_id = populated_agent
    context = agent.get_context_for_agent(user_id, "test", "irrelevant", top_k=2)
    assert context == [
        {"type": "note", "content": "second"},
        {"type": "note", "content": "third"},
    ]


def test_get_context_no_user_returns_mock(agent):
    context = agent.get_context_for_agent("unknown", "agent", "question", top_k=1)
    assert len(context) == 1
    assert context[0]["type"] in {"conversation_summary", "user_preference"}


def test_add_memory_missing_keys_raises(agent):
    with pytest.raises(ValueError) as exc:
        agent.add_memory("u1", {"type": "note"})
    assert "memory_item must contain keys" in str(exc.value)
