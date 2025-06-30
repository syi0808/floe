from __future__ import annotations

from datetime import datetime
from typing import Dict, List, Any, Union
import json


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

    # ------------------------------------------------------------------
    def compile_daily(self, agent_data: Dict[str, List[Dict[str, Any]]]) -> Dict[str, Dict[str, Any]]:
        """Wrapper for daily compilation. ``agent_data`` is assumed to contain only entries for a single day."""
        return self.compile(agent_data)

    def compile_weekly(self, agent_data: Dict[str, List[Dict[str, Any]]]) -> Dict[str, Dict[str, Any]]:
        """Wrapper for weekly compilation. ``agent_data`` is assumed to contain only entries for the week."""
        return self.compile(agent_data)

    def generate_report(
        self,
        user_id: str,
        period: str,
        agent_data: Dict[str, List[Dict[str, Any]]],
        *,
        focus: str | None = None,
        format: str = "markdown",
        mcp_client: "MCPClient" | None = None,
        notify: bool = False,
    ) -> Union[str, Dict[str, Any]]:
        """Generate a report for ``user_id`` and optionally send an MCP notification."""

        if period.lower().startswith("week"):
            compiled = self.compile_weekly(agent_data)
        else:
            compiled = self.compile_daily(agent_data)

        if format == "markdown":
            report = self._to_markdown(compiled)
        elif format == "json":
            report = self._to_json(compiled)
        else:
            raise ValueError("format must be 'markdown' or 'json'")

        if notify and mcp_client:
            payload = {
                "user_id": user_id,
                "period": period,
                "type": "insight_report",
                "content": report if format == "markdown" else json.dumps(report),
            }
            mcp_client.send_notification(payload)

        return report
