from fastapi import FastAPI, Request, Depends
from fastapi.responses import HTMLResponse
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
async def query(payload: Dict[str, Any]):
    text = payload.get("text", "")
    user_id = payload.get("user_id", "anonymous")
    intent_data = extract_intent_and_entities(text)
    response = engine.route_request(intent_data, user_id)
    return response
