from __future__ import annotations

from datetime import datetime
from typing import Dict, List, Any, Union


class InsightGenerator:
    """Compile data from agents and produce simple reports."""

    def compile(self, agent_data: Dict[str, List[Dict[str, Any]]]) -> Dict[str, Dict[str, Any]]:
        """Aggregate metrics from Schedule, Task and Health agents."""
        summary: Dict[str, Dict[str, Any]] = {}

        # ScheduleAgent aggregation ----------------------------------------
        events = agent_data.get("schedule_agent", [])
        if events:
            total_hours = 0.0
            for evt in events:
                dur = self._extract_duration_hours(evt)
                if dur is not None:
                    total_hours += dur
            summary["schedule_agent"] = {
                "count": len(events),
                "total_hours": round(total_hours, 2),
            }

        # TaskAgent aggregation --------------------------------------------
        tasks = agent_data.get("task_agent", [])
        if tasks:
            completed = sum(
                1
                for t in tasks
                if str(t.get("status", "")).lower() in {"done", "completed", "archived"}
            )
            summary["task_agent"] = {
                "count": len(tasks),
                "completed": completed,
            }

        # HealthAgent aggregation ------------------------------------------
        health_logs = agent_data.get("health_agent", [])
        if health_logs:
            sleep_scores = [h.get("sleep_score") for h in health_logs if h.get("sleep_score") is not None]
            avg_sleep = round(sum(sleep_scores) / len(sleep_scores), 2) if sleep_scores else None
            health_summary: Dict[str, Any] = {"count": len(health_logs)}
            if avg_sleep is not None:
                health_summary["avg_sleep_score"] = avg_sleep
            summary["health_agent"] = health_summary

        # Generic count for any remaining agents --------------------------
        for agent, records in agent_data.items():
            if agent in summary:
                continue
            summary[agent] = {"count": len(records)}

        return summary

    def _extract_duration_hours(self, event: Dict[str, Any]) -> float | None:
        """Attempt to derive a duration in hours from an event record."""
        if "duration_hours" in event:
            try:
                return float(event["duration_hours"])
            except (TypeError, ValueError):
                return None
        if "duration_minutes" in event:
            try:
                return float(event["duration_minutes"]) / 60.0
            except (TypeError, ValueError):
                return None
        if "start" in event and "end" in event:
            try:
                start = datetime.fromisoformat(str(event["start"]).replace("Z", "+00:00"))
                end = datetime.fromisoformat(str(event["end"]).replace("Z", "+00:00"))
            except ValueError:
                return None
            return (end - start).total_seconds() / 3600.0
        return None

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
            line = f"- **{agent}**: {stats['count']} entries"
            if agent == "schedule_agent" and "total_hours" in stats:
                line += f" ({stats['total_hours']}h scheduled)"
            if agent == "task_agent" and "completed" in stats:
                line += f", {stats['completed']} completed"
            if agent == "health_agent" and "avg_sleep_score" in stats:
                line += f", avg sleep score {stats['avg_sleep_score']}"
            lines.append(line)
        return "\n".join(lines)

    def _to_json(self, summary: Dict[str, Dict[str, Any]]) -> Dict[str, Any]:
        return {"summary": summary}
