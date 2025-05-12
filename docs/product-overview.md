# Floe – Product Overview

## Vision

Create an on‑device AI assistant that effortlessly orchestrates schedule, tasks, communication, and wellbeing through natural conversation while preserving user privacy.

## Core Values

1. **Single‑capture ▲ No Re‑entry** – collect information once and reuse everywhere.
2. **Proactive yet Ask‑to‑Act** – agents propose actions, the user approves.
3. **Local‑first Privacy** – LLM runs locally; all sync is encrypted and opt‑in.

## Primary Personas

| Persona               | Pain Point             | Desired Outcome                     |
| --------------------- | ---------------------- | ----------------------------------- |
| Busy professional     | Calendar overload      | Clear daily brief & smart reminders |
| Startup founder       | Context‑switch fatigue | Delegated triage & KPI snapshots    |
| Health‑focused worker | Burnout risk           | Insight on recovery vs. workload    |

## End‑to‑End User Flow

1. **Capture** → ConversationAgent logs intent.
2. **Plan** → OrchestratorAgent selects agent chain.
3. **Act** → Domain agents execute/schedule.
4. **Reflect** → InsightAgent summarises outcome.

## High‑Level Architecture

* **Electron host** spawns each Agent as an isolated Node process; IPC over A2A JSON messages.
* **Anthropic MCP** governs multi‑agent reasoning templates.
* **LLM backend**: llama‑cpp‑python served via OpenAI‑compatible REST.
* **Local Vector Store**: encrypted SQLite + Chroma for semantic recall.

## Integrations

Calendar (Google/Microsoft) · Email (IMAP/Gmail) · Wearables (HealthKit/Fit) · Local file index.

## Roadmap

| Version | Milestone | Highlights                                 |
| ------- | --------- | ------------------------------------------ |
| v1.0    | MVP       | Task & schedule flow, Orchestrator, Memory |
| v1.1    | Health    | Sleep & Activity modules                   |
| v1.2    | Insights  | Trend dashboard & goals                    |

## Acceptance Gates

* 95 % of “What’s next?” queries answered < 2 s.
* Daily digest delivered 20:00 local.
* Data persists across restarts without cloud.
