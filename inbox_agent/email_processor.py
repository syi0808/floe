from __future__ import annotations

from typing import Any, Dict, List, Optional
import json
import openai


def summarize_email(body_text: str, max_length: int = 150) -> str:
    """Summarize email text using an LLM."""
    try:
        client = openai.OpenAI()
        resp = client.chat.completions.create(
            model="gpt-3.5-turbo",
            messages=[
                {"role": "system", "content": "Summarize the following email text concisely."},
                {"role": "user", "content": body_text},
            ],
            max_tokens=max_length // 4,
            temperature=0.2,
        )
        return resp.choices[0].message.content.strip()
    except Exception:
        return body_text[:max_length]


def extract_email_actions(email_id: str, subject: str, body_text: str, sender: str) -> List[Dict[str, Any]]:
    """Use an LLM to extract structured actions from an email."""
    prompt = (
        "You are an assistant that extracts scheduling requests or tasks from emails. "
        "Return a JSON array of actions with keys 'action' and 'details'."
        " Possible actions are PROPOSE_SCHEDULE and CREATE_TASK."
    )
    client = openai.OpenAI()
    try:
        resp = client.chat.completions.create(
            model="gpt-3.5-turbo",
            messages=[
                {"role": "system", "content": prompt},
                {"role": "user", "content": f"Subject: {subject}\nFrom: {sender}\n{body_text}"},
            ],
            temperature=0.2,
            max_tokens=200,
        )
        content = resp.choices[0].message.content.strip()
        actions = json.loads(content)
        if isinstance(actions, list):
            return actions
    except Exception:
        pass

    # Fallback to keyword based extraction
    content = f"{subject} {body_text}".lower()
    actions: List[Dict[str, Any]] = []
    if any(word in content for word in ["meet", "schedule", "appointment", "rsvp"]):
        actions.append({"action": "PROPOSE_SCHEDULE", "details": {"text": body_text}, "source_email_id": email_id})
    if any(word in content for word in ["task", "todo", "review", "approve", "complete"]):
        actions.append({"action": "CREATE_TASK", "details": {"text": body_text}, "source_email_id": email_id})
    return actions


def process_new_email(
    user_id: str,
    email_data: Dict[str, Any],
    schedule_agent: Any = None,
    task_agent: Any = None,
    memory_manager: Any = None,
    orchestrator: Any = None,
) -> None:
    """Process a new email by summarizing and routing actions to other agents."""
    body_text = email_data.get("body_text", "")
    summary = summarize_email(body_text)
    email_id = email_data.get("id")
    if memory_manager:
        memory_manager.add_memory(
            user_id,
            {"type": "email_summary", "content": summary, "email_id": email_id},
        )
        for att in email_data.get("attachments", []):
            memory_manager.add_memory(
                user_id,
                {
                    "type": "email_attachment",
                    "email_id": email_id,
                    "content": att,
                },
            )

    actions = extract_email_actions(
        email_id=email_id or "",
        subject=email_data.get("subject", ""),
        body_text=body_text,
        sender=email_data.get("sender", ""),
    )

    for action in actions:
        intent_map = {
            "PROPOSE_SCHEDULE": "extract_schedule_info",
            "CREATE_TASK": "task_management",
        }
        intent = intent_map.get(action["action"])
        if orchestrator and intent:
            orchestrator.route_request(
                {"intent": intent, "entities": action["details"]},
                user_id,
            )
        else:
            if action["action"] == "PROPOSE_SCHEDULE" and schedule_agent:
                schedule_agent.process(action["details"], user_id)
            elif action["action"] == "CREATE_TASK" and task_agent:
                task_agent.process(action["details"], user_id)

