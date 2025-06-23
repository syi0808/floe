from __future__ import annotations

from typing import Dict, List, Any, Union


class InsightGenerator:
    """Compile data from agents and produce simple reports."""

    def compile(self, agent_data: Dict[str, List[Dict[str, Any]]]) -> Dict[str, Dict[str, Any]]:
        """Aggregate counts of records from each agent."""
        summary: Dict[str, Dict[str, Any]] = {}
        for agent, records in agent_data.items():
            summary[agent] = {"count": len(records)}
        return summary

    def generate_summary(
        self, agent_data: Dict[str, List[Dict[str, Any]]], format: str = "markdown"
    ) -> Union[str, Dict[str, Any]]:
        """Return a report summarising ``agent_data`` in the requested format."""
        compiled = self.compile(agent_data)
        if format == "markdown":
            return self._to_markdown(compiled)
        if format == "json":
            return self._to_json(compiled)
        raise ValueError("format must be 'markdown' or 'json'")

    def _to_markdown(self, summary: Dict[str, Dict[str, Any]]) -> str:
        lines = ["# Insight Report", ""]
        for agent, stats in summary.items():
            lines.append(f"- **{agent}**: {stats['count']} entries")
        return "\n".join(lines)

    def _to_json(self, summary: Dict[str, Dict[str, Any]]) -> Dict[str, Any]:
        return {"summary": summary}
