import pytest
from insight_agent.insight_generator import InsightGenerator


def test_compile_counts():
    gen = InsightGenerator()
    data = {
        "schedule_agent": [{"id": 1}, {"id": 2}],
        "task_agent": [],
    }
    summary = gen.compile(data)
    assert summary == {
        "schedule_agent": {"count": 2},
        "task_agent": {"count": 0},
    }


def test_generate_summary_markdown():
    gen = InsightGenerator()
    result = gen.generate_summary({"agent": [{}, {}]}, format="markdown")
    assert result.startswith("# Insight Report")
    assert "**agent**" in result
    assert "2 entries" in result


def test_generate_summary_json():
    gen = InsightGenerator()
    result = gen.generate_summary({"a": [{}]}, format="json")
    assert result == {"summary": {"a": {"count": 1}}}


@pytest.mark.parametrize("fmt", ["bad", "xml"])
def test_generate_summary_invalid_format(fmt):
    gen = InsightGenerator()
    with pytest.raises(ValueError):
        gen.generate_summary({}, format=fmt)
