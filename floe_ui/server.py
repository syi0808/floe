import logging
import os

from fastapi import FastAPI, Request, Depends, HTTPException
from fastapi.responses import HTMLResponse, JSONResponse
from fastapi.templating import Jinja2Templates
from orchestrator_agent.orchestrator_core import OrchestrationEngine
from orchestrator_agent.intent_analyzer import extract_intent_and_entities
from schedule_agent.schedule_agent import ScheduleAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent
from inbox_agent.inbox_agent import InboxAgent
from health_agent.health_agent import HealthAgent
from insight_agent.insight_agent import InsightAgent
from typing import Dict, Any

app = FastAPI()
templates = Jinja2Templates(directory="floe_ui/templates")

logger = logging.getLogger(__name__)
logging.basicConfig(level=logging.INFO)

# Simple token-based authentication
AUTH_TOKEN = os.getenv("FLOE_UI_TOKEN", "changeme")

async def verify_token(request: Request):
    auth_header = request.headers.get("Authorization")
    expected = f"Bearer {AUTH_TOKEN}"
    if auth_header != expected:
        raise HTTPException(status_code=401, detail={"status": "error", "message": "Unauthorized"})

# Initialize orchestrator engine similarly to CLI
memory_manager = MemoryManagerAgent()
engine = OrchestrationEngine(memory_manager_client=memory_manager)
engine.register_agent(ScheduleAgent())
engine.register_agent(InboxAgent())
engine.register_agent(HealthAgent())
engine.register_agent(InsightAgent())

@app.get("/", response_class=HTMLResponse)
async def index(request: Request):
    return templates.TemplateResponse("index.html", {"request": request})

@app.post("/query")
async def query(payload: Dict[str, Any], _: None = Depends(verify_token)):
    text = payload.get("text", "")
    user_id = payload.get("user_id", "anonymous")
    try:
        intent_data = extract_intent_and_entities(text)
        response = engine.route_request(intent_data, user_id)
        return response
    except Exception as e:  # pragma: no cover - logging side effect
        logger.exception("OrchestrationEngine call failed")
        return JSONResponse(
            status_code=500,
            content={
                "status": "error",
                "message": "Failed to process request",
                "data": {"detail": str(e)},
                "source_agent": "FloeUI",
            },
        )
