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

        goal_progress = self._goal_progress(agent_data)
        if goal_progress:
            summary["goals"] = goal_progress

        trends = self._compute_trends(agent_data)
        if trends:
            summary["trends"] = trends

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

    def _goal_progress(self, agent_data: Dict[str, List[Dict[str, Any]]]) -> Dict[str, float]:
        """Return goal progress percentages keyed by goal id."""
        goals = agent_data.get("goals") or agent_data.get("goal_agent") or []
        progress: Dict[str, float] = {}
        for g in goals:
            gid = g.get("id") or g.get("goal_id")
            if not gid:
                continue
            if "target" in g and "current" in g:
                try:
                    pct = (float(g["current"]) / float(g["target"])) * 100.0
                except (TypeError, ValueError, ZeroDivisionError):
                    continue
            elif "progress" in g:
                try:
                    pct = float(g["progress"])
                except (TypeError, ValueError):
                    continue
            else:
                continue
            progress[gid] = round(pct, 2)
        return progress

    def _compute_trends(self, agent_data: Dict[str, List[Dict[str, Any]]]) -> Dict[str, Dict[str, Any]]:
        """Return simple time-series metrics grouped by date."""
        trends: Dict[str, Dict[str, Any]] = {}

        events = agent_data.get("schedule_agent", [])
        hours_by_day: Dict[str, float] = {}
        for evt in events:
            dt = evt.get("start") or evt.get("date")
            if not dt:
                continue
            try:
                day = datetime.fromisoformat(str(dt).replace("Z", "+00:00")).date().isoformat()
            except ValueError:
                continue
            hrs = self._extract_duration_hours(evt) or 0.0
            hours_by_day[day] = round(hours_by_day.get(day, 0.0) + hrs, 2)
        if hours_by_day:
            trends["schedule_hours"] = hours_by_day

        tasks = agent_data.get("task_agent", [])
        completed_by_day: Dict[str, int] = {}
        for t in tasks:
            status = str(t.get("status", "")).lower()
            if status not in {"done", "completed", "archived"}:
                continue
            dt = t.get("completed_at") or t.get("date")
            if not dt:
                continue
            try:
                day = datetime.fromisoformat(str(dt).replace("Z", "+00:00")).date().isoformat()
            except ValueError:
                continue
            completed_by_day[day] = completed_by_day.get(day, 0) + 1
        if completed_by_day:
            trends["completed_tasks"] = completed_by_day

        health_logs = agent_data.get("health_agent", [])
        sleep_by_day: Dict[str, List[float]] = {}
        for h in health_logs:
            if h.get("sleep_score") is None:
                continue
            dt = h.get("date") or h.get("timestamp")
            if not dt:
                continue
            try:
                day = datetime.fromisoformat(str(dt).replace("Z", "+00:00")).date().isoformat()
            except ValueError:
                continue
            sleep_by_day.setdefault(day, []).append(float(h["sleep_score"]))
        if sleep_by_day:
            trends["avg_sleep_score"] = {d: round(sum(scores) / len(scores), 2) for d, scores in sleep_by_day.items()}

        return trends

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
            if agent in {"goals", "trends"}:
                continue
            line = f"- **{agent}**: {stats['count']} entries"
            if agent == "schedule_agent" and "total_hours" in stats:
                line += f" ({stats['total_hours']}h scheduled)"
            if agent == "task_agent" and "completed" in stats:
                line += f", {stats['completed']} completed"
            if agent == "health_agent" and "avg_sleep_score" in stats:
                line += f", avg sleep score {stats['avg_sleep_score']}"
            lines.append(line)

        if "goals" in summary:
            lines.append("\n## Goal Progress")
            for gid, pct in summary["goals"].items():
                lines.append(f"- {gid}: {pct}%")

        if "trends" in summary:
            lines.append("\n## Trends")
            for tname, series in summary["trends"].items():
                lines.append(f"- {tname}: {len(series)} points")

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

    # ------------------------------------------------------------------
    def generate_daily_report(
        self,
        user_id: str,
        agent_data: Dict[str, List[Dict[str, Any]]],
        *,
        focus: str | None = None,
        format: str = "markdown",
        mcp_client: "MCPClient" | None = None,
        notify: bool = True,
    ) -> Union[str, Dict[str, Any]]:
        """Convenience wrapper for generating a daily report.

        This calls :meth:`generate_report` with ``period='daily'``. ``notify``
        defaults to ``True`` so that a notification is sent when an
        ``mcp_client`` is provided.
        """

        return self.generate_report(
            user_id=user_id,
            period="daily",
            agent_data=agent_data,
            focus=focus,
            format=format,
            mcp_client=mcp_client,
            notify=notify,
        )

    def generate_weekly_report(
        self,
        user_id: str,
        agent_data: Dict[str, List[Dict[str, Any]]],
        *,
        focus: str | None = None,
        format: str = "markdown",
        mcp_client: "MCPClient" | None = None,
        notify: bool = True,
    ) -> Union[str, Dict[str, Any]]:
        """Convenience wrapper for generating a weekly report.

        Works the same as :meth:`generate_daily_report` but with
        ``period='weekly'``.
        """

        return self.generate_report(
            user_id=user_id,
            period="weekly",
            agent_data=agent_data,
            focus=focus,
            format=format,
            mcp_client=mcp_client,
            notify=notify,
        )
