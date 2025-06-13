# Floe AI Assistant - Implementation Plan

This document provides a detailed, step-by-step implementation plan for the Floe AI assistant, based on the information outlined in `docs/work_plan.md`.

## Table of Contents

- [1. Introduction](#1-introduction)
- [2. Overall Architecture](#2-overall-architecture)
- [3. Agent Implementation Details](#3-agent-implementation-details)
  - [3.1. OrchestratorAgent](#31-orchestratoragent)
  - [3.2. MemoryManagerAgent](#32-memorymanageragent)
  - [3.3. ConversationAgent](#33-conversationagent)
  - [3.4. ScheduleAgent (SchedulerAgent)](#34-scheduleagent-scheduleragent)
  - [3.5. TaskAgent (TaskManagerAgent)](#35-taskagent-taskmanageragent)
  - [3.6. InboxAgent](#36-inboxagent)
  - [3.7. HealthAgent](#37-healthagent)
  - [3.8. InsightAgent](#38-insightagent)
- [4. MCP Server Integration Plan](#4-mcp-server-integration-plan)
- [5. Testing Strategy](#5-testing-strategy)
- [6. Deployment Strategy](#6-deployment-strategy)

## 1. Introduction

Floe is a natural language-based AI assistant built with a modular, agent-based architecture. Each agent is responsible for a specific domain, ensuring scalability and adherence to the single responsibility principle.

**Core Technologies:**
- Python
- OpenAI's Agents SDK
- MCP Server Integration

## 2. Overall Architecture

Floe utilizes a multi-agent system orchestrated by a central agent. Key components include:
- **OrchestratorAgent**: Routes user commands to appropriate agents.
- **MemoryManagerAgent**: Manages short-term and long-term memory.
- **Specialized Agents**: Handle tasks related to conversation, scheduling, tasks, inbox management, health, and insights.

## 3. Agent Implementation Details

### 3.1. OrchestratorAgent

**From work_plan.md:**
- **Core Role**: User command analysis and routing.
- **Key Features**:
    - Intent analysis from natural language.
    - Sequential and parallel orchestration of sub-agents.
    - Integration with MemoryManagerAgent for contextual information.
    - Aggregation of responses from multiple agents.
- **OpenAI Agents SDK Usage**: For intent recognition and complex decision-making.
- **MCP Integration**: To receive user commands and dispatch tasks to other services if needed.

**Implementation Details:**

1.  **Intent Analysis Module (`intent_analyzer.py`):**
    *   **Function**: `analyze_intent(user_query: str, conversation_context: Optional[dict]) -> dict`:
        *   Utilizes OpenAI's function calling or a dedicated classification model to identify primary intent (e.g., `CREATE_SCHEDULE`, `ADD_TASK`, `SEND_MESSAGE`, `QUERY_HEALTH_DATA`).
        *   Extracts key entities (e.g., dates, times, task descriptions, recipients).
        *   Returns a structured dictionary: `{'intent': '...', 'entities': {...}, 'confidence': 0.95}`.
    *   **Consideration**: Explore caching mechanisms for common queries.

2.  **Orchestration Logic (`orchestrator_core.py`):**
    *   **Class**: `OrchestrationEngine`:
        *   `constructor(memory_manager_agent_client, available_agents_map)`
        *   **Method**: `route_request(intent_data: dict, user_id: str) -> AgentResponse`:
            *   Retrieves relevant short-term memory/context from `MemoryManagerAgent` using `user_id`.
            *   Based on `intent_data['intent']`, determines the target agent(s) from `available_agents_map`.
            *   **Sequential Orchestration**: For dependent tasks (e.g., get contact details then compose message), call agents sequentially.
            *   **Parallel Orchestration**: For independent sub-tasks (e.g., fetch weather and news), dispatch calls concurrently (e.g., using `asyncio`).
            *   Handles errors and timeouts from agent calls.
    *   **Response Aggregation**:
        *   Develop a strategy for combining responses if multiple agents are invoked (e.g., summarization, structured list).
        *   Define a standard `AgentResponse` format: `{'status': 'success'/'error', 'data': ..., 'message': ..., 'source_agent': 'OrchestratorAgent'}`.

3.  **OpenAI SDK Integration:**
    *   Leverage `openai.ChatCompletion` with function definitions for robust intent and entity extraction.
    *   Explore Agents SDK features for more complex conversational flows if needed, though primary routing might be rule-based post-intent-extraction.

4.  **MCP Integration Points:**
    *   **Receive User Commands**:
        *   Endpoint: `POST /mcp/commands`
        *   Request Schema: `{'user_id': '...', 'query': '...', 'timestamp': '...'}`
        *   Response Schema: (Initially, ack, then async update or direct response via other channel)
    *   **Dispatch Tasks (if external services are orchestrated via MCP):**
        *   Endpoint: (To be defined based on MCP capabilities, e.g., `POST /mcp/invoke_service`)
        *   Request Schema: `{'service_name': '...', 'payload': {...}}`

### 3.2. MemoryManagerAgent

**From work_plan.md:**
- **Core Role**: Long-term and short-term memory management.
- **Key Features**:
    - Vectorization and storage of diverse memory types (conversations, schedules, tasks, etc.).
    - Semantic search for memory retrieval.
    - TTL and recency-based prioritization.
    - Automatic context injection for other agents.
- **OpenAI Agents SDK Usage**: For embedding generation and semantic search capabilities.
- **MCP Integration**: For persisting and retrieving memory data from a central data store.

**Implementation Details:**

1.  **Memory Storage Module (`memory_store.py`):**
    *   **Data Structures**:
        *   Define Pydantic models for different memory types: `ConversationMemory`, `ScheduleMemory`, `TaskMemory`, `UserPreference`, `DocumentMemory`. Each should include `user_id`, `timestamp`, `ttl_seconds`, `content`, and `vector_embedding`.
    *   **Vectorization**:
        *   **Function**: `get_embedding(text: str, model: str = "text-embedding-ada-002") -> List[float]`:
            *   Uses OpenAI API to generate embeddings.
    *   **Storage Backend**:
        *   Initial: Use a local vector database (e.g., FAISS, ChromaDB) for development.
        *   Production: Integrate with a robust vector DB service via MCP or directly.
    *   **CRUD Operations**:
        *   `add_memory(user_id: str, memory_item: BaseMemoryModel)`: Adds item, generates embedding, stores both.
        *   `get_memory(memory_id: str) -> Optional[BaseMemoryModel]`: Retrieves a specific memory item.
        *   `update_memory(memory_id: str, updates: dict)`
        *   `delete_memory(memory_id: str)`

2.  **Memory Retrieval Module (`memory_retriever.py`):**
    *   **Semantic Search**:
        *   **Function**: `search_memories(user_id: str, query_text: str, top_k: int = 5, filter_types: Optional[List[str]] = None) -> List[BaseMemoryModel]`:
            *   Generates embedding for `query_text`.
            *   Performs similarity search in the vector DB for the given `user_id`.
            *   Applies filters for `memory_types` (e.g., only 'ConversationMemory').
            *   Implements TTL logic: Exclude expired memories.
            *   Implements recency prioritization: Optionally boost scores of recent memories.
    *   **Contextual Retrieval for Agents**:
        *   **Function**: `get_context_for_agent(user_id: str, agent_name: str, current_query: str, max_tokens: int = 1000) -> List[BaseMemoryModel]`:
            *   Retrieves relevant memories based on agent type and current query.
            *   May involve a combination of recent conversation history and semantically similar items.

3.  **Automatic Context Injection Strategy:**
    *   The `OrchestratorAgent` will primarily call `MemoryManagerAgent` to fetch context.
    *   Alternatively, individual agents could directly query for specific memory types they need, facilitated by a client library for `MemoryManagerAgent`.

4.  **OpenAI SDK Integration:**
    *   Primarily `openai.Embedding.create()` for generating embeddings.
    *   Consider SDK features if it offers higher-level abstractions for managing vectorized data or interfacing with vector stores in the future.

5.  **MCP Integration Points:**
    *   **Persist/Retrieve Memory Data (if MCP provides a central data store):**
        *   Endpoint: `POST /mcp/memory/{user_id}` (for adding)
        *   Request Schema: `{ 'type': 'conversation'/'task'/..., 'data': {...}, 'ttl_seconds': Optional[int] }`
        *   Endpoint: `GET /mcp/memory/{user_id}/search`
        *   Request Schema: `{ 'query_text': '...', 'top_k': 5, 'filter_types': ['task'] }`
        *   Response Schema: `List[{'id': '...', 'type': '...', 'data': {...}, 'score': 0.89}]`
    *   **Note**: If MCP doesn't offer a vector DB, this agent manages its own, and MCP integration might be minimal or for backup/archival.

### 3.3. ConversationAgent

**From work_plan.md:**
- **Core Role**: Natural language interaction and context maintenance.
- **Key Features**:
    - Handling text/voice input.
    - Dialogue flow management and inference.
    - Clarification questions.
    - Task interruption and re-engagement.
    - Cached intent processing for quick responses.
- **OpenAI Agents SDK Usage**: For natural language understanding, dialogue state tracking.
- **MCP Integration**: For sending/receiving messages via various communication channels.

**Implementation Details:**

1.  **Input Handling Module (`input_handler.py`):**
    *   **Text Input**:
        *   **Function**: `process_text_input(text: str, user_id: str, session_id: str) -> AgentResponse`:
            *   Receives raw text from user.
            *   (Future) Potentially integrates with a Speech-to-Text service if voice input comes as pre-transcribed text.
    *   **Voice Input (Future Scope - initial focus on text):**
        *   If direct voice input is supported: Integrate with a Speech-to-Text (STT) service (e.g., OpenAI Whisper API, Google Cloud Speech-to-Text).
        *   **Function**: `process_audio_input(audio_data: bytes, user_id: str, session_id: str) -> AgentResponse`: Transcribes audio then processes as text.

2.  **Dialogue Management Module (`dialogue_manager.py`):**
    *   **Class**: `DialogueFlow`:
        *   `constructor(memory_manager_client, orchestrator_client)`
        *   **State Tracking**: Maintain dialogue state per `session_id` (e.g., current intent, slots filled, waiting_for_clarification). Store state via MemoryManagerAgent for persistence across sessions if needed.
        *   **Method**: `handle_message(user_id: str, session_id: str, message_content: str) -> AgentResponse`:
            *   Retrieves dialogue history/context from `MemoryManagerAgent`.
            *   Calls `OrchestratorAgent` to get intent and entities.
            *   **Clarification Logic**: If intent/entities are ambiguous or incomplete:
                *   Generate clarification questions (e.g., "Do you mean schedule for tomorrow or next week?").
                *   Update state to `WAITING_FOR_CLARIFICATION`.
            *   **Task Interruption & Re-engagement**:
                *   If user initiates a new, unrelated query mid-task: Save current task state (e.g., partially filled schedule), handle new query.
                *   Provide mechanisms to resume previous task (e.g., "Do you want to continue creating that schedule?").
            *   **Response Generation**: Formulate natural language responses. Could use simple templates or an LLM for more dynamic responses.
                *   **Function**: `generate_response(agent_action_result: dict, dialogue_state: dict) -> str`.

3.  **Cached Intent Processing:**
    *   Before calling OrchestratorAgent, check a local cache (or `MemoryManagerAgent` for "quick responses") for very common, simple commands (e.g., "hello", "thank you") to provide faster replies without full orchestration.
    *   Cache key: `(user_id, normalized_query_text)`.

4.  **OpenAI SDK Integration:**
    *   NLU: Can use `openai.ChatCompletion` for understanding follow-up messages in context, especially if not using full Orchestrator intent analysis for every turn.
    *   Response Generation: `openai.ChatCompletion` can be used to generate more natural and contextually aware responses if simple templating is insufficient.
    *   Agents SDK: Explore for more robust dialogue state tracking and turn-by-turn interaction models provided by the SDK.

5.  **MCP Integration Points:**
    *   **Receive Messages (from various channels via MCP):**
        *   Endpoint: `POST /mcp/conversation/{user_id}/message`
        *   Request Schema: `{'session_id': '...', 'channel_type': 'text'/'voice_transcript', 'content': '...', 'timestamp': '...'}`
        *   Response Schema: (Async) MCP pushes ConversationAgent's reply back to the originating channel.
    *   **Send Messages (to various channels via MCP):**
        *   Endpoint: `POST /mcp/send_reply`
        *   Request Schema: `{'user_id': '...', 'session_id': '...', 'channel_type': 'text', 'content': '...', 'target_details': {...}}` (target_details for specific channel info)

### 3.4. ScheduleAgent (SchedulerAgent)

**From work_plan.md:**
- **Core Role**: Schedule creation, conflict detection, and time recommendation.
- **Key Features**:
    - Parsing schedule details from natural language.
    - Integration with Google/Microsoft Calendars.
    - Meeting time recommendations based on attendee availability.
    - Handling complex and recurring schedules.
    - Schedule summarization.
- **OpenAI Agents SDK Usage**: For natural language parsing of dates, times, and locations.
- **MCP Integration**: To sync with external calendar services and potentially with other users' Floe instances for group scheduling.

**Implementation Details:**

1.  **Natural Language Parsing Module (`schedule_parser.py`):**
    *   **Function**: `parse_schedule_request(natural_language_query: str, user_timezone: str) -> dict`:
        *   Identifies event title, participants, date/time expressions (e.g., "next Monday at 3 PM", "tomorrow morning"), duration, location, recurrence patterns (e.g., "every week").
        *   Uses libraries like `dateparser` for flexible date/time string interpretation, considering `user_timezone`.
        *   Leverages OpenAI function calling or entity extraction if complex phrases need robust parsing (e.g., "Schedule a meeting with John and Jane for next week to discuss the project budget for about 1 hour").
        *   Returns structured data: `{'title': '...', 'attendees': ['email1', 'name2'], 'start_time_utc': '...', 'end_time_utc': '...', 'location': '...', 'recurrence_rule': 'RRULE:...'}`.

2.  **Calendar Integration Module (`calendar_connectors.py`):**
    *   **Base Class**: `AbstractCalendarConnector`:
        *   Defines interface: `create_event`, `read_events(start_date, end_date)`, `update_event`, `delete_event`, `get_free_busy(user_ids, start_date, end_date)`.
    *   **Concrete Classes**:
        *   `GoogleCalendarConnector(AbstractCalendarConnector)`: Implements methods using Google Calendar API (OAuth2 for authentication).
        *   `MicrosoftCalendarConnector(AbstractCalendarConnector)`: Implements methods using Microsoft Graph API (OAuth2).
    *   User credentials (OAuth tokens) should be securely managed, possibly via MCP or a dedicated secrets manager.

3.  **Scheduling Logic Module (`scheduler_core.py`):**
    *   **Function**: `create_schedule_entry(user_id: str, parsed_schedule_data: dict) -> AgentResponse`:
        *   Validates `parsed_schedule_data`.
        *   Checks for conflicts in the user's calendar using the appropriate `CalendarConnector`.
        *   If no conflicts (or user confirms override), creates the event via the connector.
        *   Stores a reference or copy of the event in `MemoryManagerAgent` if needed for quick lookups by Floe.
    *   **Function**: `find_meeting_times(organizer_id: str, required_attendees: List[str], optional_attendees: List[str], duration_minutes: int, time_window_start, time_window_end) -> List[dict]`:
        *   Fetches free-busy information for all attendees (requires their Floe instances to grant access or direct calendar integration).
        *   Identifies common available slots.
        *   Returns a list of suggested time slots: `[{'start_time_utc': '...', 'end_time_utc': '...'}]`.
    *   **Conflict Resolution**: If a direct creation causes a conflict, suggest alternative times or ask user for confirmation.
    *   **Recurring Events**: Translate `recurrence_rule` into calendar-specific format.

4.  **Schedule Summarization Module (`schedule_summary.py`):**
    *   **Function**: `get_schedule_summary(user_id: str, date_or_period: str) -> str`:
        *   Fetches events for the specified day/week from the calendar.
        *   Formats into a concise natural language summary (e.g., "Today you have 3 meetings: Project Sync at 10 AM, Lunch with team at 1 PM...").

5.  **OpenAI SDK Integration:**
    *   Use for the NLP aspects in `schedule_parser.py` if simple entity extraction is insufficient. Function calling is a strong candidate here for structured output.

6.  **MCP Integration Points:**
    *   **Sync with External Calendars (if MCP brokers connections or stores credentials):**
        *   MCP might handle OAuth flows and token refresh for Google/Microsoft.
        *   ScheduleAgent would request calendar operations via MCP: `POST /mcp/calendar/{user_id}/events`
    *   **Group Scheduling (cross-user free/busy):**
        *   MCP could facilitate querying availability of other Floe users: `GET /mcp/users/availability?user_ids=id1,id2&start=...&end=...`
    *   **Notifications**: MCP could be used to send event reminders or updates to users.

### 3.5. TaskAgent (TaskManagerAgent)

**From work_plan.md:**
- **Core Role**: Task creation, structuring, prioritization, and reminders.
- **Key Features**:
    - Structuring tasks from natural language commands.
    - Extracting action items from texts (emails, meeting notes).
    - Priority calculation based on due dates and importance.
    - Calendar blocking integration.
    - Automated linking of tasks and schedules.
- **OpenAI Agents SDK Usage**: For NLP-based task extraction and understanding dependencies.
- **MCP Integration**: To store tasks and link with calendar entries.

**Implementation Details:**

1.  **Task Parsing Module (`task_parser.py`):**
    *   **Function**: `parse_task_request(natural_language_query: str, context_document: Optional[str] = None) -> dict`:
        *   Identifies task description, due dates/times, priority indicators (e.g., "urgent", "important"), assignees (if applicable in a team context, though primarily for self), project/category.
        *   If `context_document` (e.g., email body, meeting transcript) is provided, scan for action items (e.g., "I will send the report by Friday", "Can you follow up on X?").
        *   Uses OpenAI function calling or entity extraction for robust NLP.
        *   Returns structured data: `{'description': '...', 'due_date_utc': '...', 'priority': 1-4, 'project': '...', 'source': 'nlp'/'email'}`.
    *   **Action Item Extraction**:
        *   May require specific prompts for LLMs if extracting from larger texts, focusing on identifying commitments or requests.

2.  **Task Management Module (`task_core.py`):**
    *   **Data Structure**: `TaskItem` (Pydantic model): `id`, `user_id`, `description`, `created_at`, `due_date_utc`, `completed_at`, `priority`, `status` (e.g., 'todo', 'in-progress', 'done'), `project_tags`, `linked_schedule_id`.
    *   **Storage**: Store `TaskItem` objects using `MemoryManagerAgent` (for vector search on description) and/or a dedicated task database via MCP.
    *   **CRUD Operations**: `create_task`, `get_task`, `update_task` (status, due date), `delete_task`, `list_tasks` (with filtering by status, project, due date range).
    *   **Priority Calculation**:
        *   Initial priority from `parse_task_request`.
        *   Could implement Eisenhower matrix logic (Urgent/Important) or a scoring system based on due date proximity and stated importance.
    *   **Reminders**:
        *   Logic to trigger reminders based on due dates (e.g., 1 day before, morning of). This might involve a separate scheduler process or MCP's notification capabilities.

3.  **Calendar Blocking Integration (`task_calendar_linker.py`):**
    *   **Function**: `block_time_for_task(user_id: str, task_id: str, task_description: str, estimated_duration_hours: int, preferred_time_window: Optional[dict]) -> Optional[str]`:
        *   Optionally interacts with `ScheduleAgent` to find and book a time slot for focused work on a task.
        *   `preferred_time_window` could be "today", "tomorrow morning", etc.
        *   Returns the ID of the created calendar event if successful.
    *   Updates `TaskItem` with `linked_schedule_id`.

4.  **Automated Linking (Tasks & Schedules):**
    *   When a task is created with a due date, it can be optionally displayed on a "task calendar" view.
    *   If a meeting (from ScheduleAgent) has action items identified (perhaps by InboxAgent processing meeting notes), these can be automatically suggested as tasks.

5.  **OpenAI SDK Integration:**
    *   Critical for `task_parser.py` for both direct task commands ("Remind me to buy milk") and action item extraction ("John will follow up on the slides").
    *   Function calling for structured output of task details is highly recommended.

6.  **MCP Integration Points:**
    *   **Task Storage & Retrieval (if MCP has a dedicated task service or generic DB):**
        *   `POST /mcp/tasks/{user_id}`: Create a new task.
        *   Request: `{ 'description': '...', 'due_date_utc': '...', ... }`
        *   `GET /mcp/tasks/{user_id}?status=todo&project=X`: List tasks.
    *   **Notifications/Reminders**:
        *   TaskAgent determines *when* a reminder is needed.
        *   `POST /mcp/notifications` : Send reminder content to MCP for delivery.
        *   Request: `{'user_id': '...', 'type': 'task_reminder', 'message': 'Reminder: Buy milk is due today.'}`
    *   **Link with Calendar Entries (if MCP manages calendar event IDs):**
        *   TaskAgent would inform MCP of links: `POST /mcp/links {'type': 'task_to_calendar', 'task_id': '...', 'calendar_event_id': '...'}`.

### 3.6. InboxAgent

**From work_plan.md:**
- **Core Role**: Analyzing and extracting information from emails, notifications, and external messages.
- **Key Features**:
    - Gmail/Outlook integration.
    - LLM-based email summarization.
    - Recognizing schedule proposals/requests and forwarding to ScheduleAgent.
    - Extracting tasks/attachments and forwarding to TaskAgent.
    - Archiving meeting invites/files to MemoryManagerAgent.
- **OpenAI Agents SDK Usage**: For email content analysis, summarization, and intent extraction.
- **MCP Integration**: To connect with email services and route extracted information.

**Implementation Details:**

1.  **Email Integration Module (`email_connectors.py`):**
    *   **Base Class**: `AbstractEmailConnector`:
        *   Defines interface: `list_emails(max_count, since_timestamp)`, `get_email_body(email_id)`, `get_attachments(email_id)`.
    *   **Concrete Classes**:
        *   `GmailConnector(AbstractEmailConnector)`: Uses Gmail API (OAuth2).
        *   `OutlookConnector(AbstractEmailConnector)`: Uses Microsoft Graph API (OAuth2).
    *   Handles new email detection (e.g., polling, push notifications if supported by API and MCP).
    *   User credentials (OAuth tokens) managed securely (via MCP or secrets manager).

2.  **Email Processing Module (`email_processor.py`):**
    *   **Function**: `process_new_email(user_id: str, email_data: dict) -> None`:
        *   `email_data` includes: `{'id': '...', 'sender': '...', 'subject': '...', 'body_text': '...', 'received_at': '...', 'attachments': [...]}`.
        *   **Summarization**:
            *   `summarize_email(body_text: str, max_length: int = 150) -> str`: Uses an LLM (e.g., OpenAI `gpt-3.5-turbo`) with a prompt like "Summarize this email in under [max_length] characters, focusing on key information and action items: [body_text]".
            *   Store summary with `MemoryManagerAgent` linked to the email.
        *   **Intent/Entity Extraction (from email body/subject):**
            *   `extract_email_actions(email_id: str, subject: str, body_text: str, sender: str) -> List[dict]`:
                *   Uses LLM (function calling is ideal) to identify:
                    *   Schedule proposals: "Can we meet next Tuesday?" -> Forward to `ScheduleAgent` (`{'action': 'PROPOSE_SCHEDULE', 'details': {...}, 'source_email_id': email_id}`).
                    *   Task assignments/requests: "Please send the report." -> Forward to `TaskAgent` (`{'action': 'CREATE_TASK', 'details': {...}, 'source_email_id': email_id}`).
                    *   Meeting invites (.ics files): Parse and forward to `ScheduleAgent`.
                    *   Important attachments/documents: Forward file reference/content to `MemoryManagerAgent` for archival and potential indexing. (`{'action': 'ARCHIVE_DOCUMENT', 'file_info': {...}, 'source_email_id': email_id}`).
        *   Calls appropriate agents based on extracted actions.

3.  **Attachment Handling:**
    *   If attachments are present, download them temporarily for analysis or store them via MCP if a central file store exists.
    *   For document types (PDF, DOCX), consider text extraction (e.g., using `pypdf2`, `python-docx`) before sending to `MemoryManagerAgent` or for analysis.

4.  **Notification Processing (Beyond Email):**
    *   This agent could be extended to process notifications from other platforms if MCP routes them here. The processing logic would be similar: summarize, extract actions.

5.  **OpenAI SDK Integration:**
    *   Primary use for summarization and complex intent/entity extraction from email bodies.
    *   `openai.ChatCompletion` with carefully crafted prompts and function definitions.

6.  **MCP Integration Points:**
    *   **Email Service Connection (if MCP brokers connections or stores credentials):**
        *   MCP handles OAuth and token management.
        *   InboxAgent requests email data via MCP: `GET /mcp/email/{user_id}/inbox?count=10`
    *   **Receiving New Email Notifications (if MCP supports push):**
        *   Endpoint on InboxAgent: `POST /inboxagent/notify_new_email`
        *   Request from MCP: `{'user_id': '...', 'email_id': '...'}` (InboxAgent then fetches details).
    *   **Routing Extracted Information (delegating to other agents via direct calls or MCP):**
        *   If via MCP: `POST /mcp/invoke_agent/{agent_name}`
        *   Payload: `{'user_id': '...', 'action_type': 'CREATE_TASK', 'data': {...}, 'source': {'type': 'email', 'id': '...'}}`
    *   **File Storage (for attachments, via MCP):**
        *   `POST /mcp/files/{user_id}` with attachment data.

### 3.7. HealthAgent

**From work_plan.md:**
- **Core Role**: Automating health management (sleep, diet, exercise, stress).
- **Sub-Modules**:
    - **SleepModule**: Sleep tracking, deficit warnings, recovery suggestions.
    - **NutritionModule**: Meal logging, nutrient tracking, meal reminders.
    - **ActivityModule**: Exercise logging, routine suggestions.
    - **WellnessModule**: Stress/burnout analysis, recovery routine recommendations.
- **Key Features**:
    - Predicting and confirming sleep/meal times based on user's schedule.
    - Wearable device integration.
    - Overwork detection and alerts.
    - Weekly health summary reports.
- **OpenAI Agents SDK Usage**: For pattern recognition in health data and generating personalized suggestions.
- **MCP Integration**: To collect data from health trackers and store user health profiles.

**Implementation Details:**

1.  **Core Health Data Model (`health_models.py`):**
    *   Define Pydantic models for:
        *   `SleepRecord(user_id, start_time_utc, end_time_utc, quality_score, source)`
        *   `MealRecord(user_id, timestamp_utc, description, calories, protein_g, carbs_g, fat_g, source)`
        *   `ActivityRecord(user_id, start_time_utc, duration_minutes, type, intensity, calories_burned, source)`
        *   `WellnessLog(user_id, timestamp_utc, stress_level, mood, notes, source)`
    *   `source` can be 'manual_entry', 'wearable_garmin', 'user_prediction_confirmation', etc.

2.  **Wearable Device Integration Module (`wearable_connectors.py`):**
    *   **Strategy**: Likely via MCP if it supports health data aggregation (e.g., from Google Fit, Apple HealthKit via partner integrations).
    *   If direct integration:
        *   `GarminConnector`, `FitbitConnector`, etc., using their respective APIs (OAuth).
        *   Focus on fetching sleep, activity, heart rate data.
    *   Data fetched would be transformed into the `HealthRecord` models.

3.  **Prediction & Confirmation Logic (`health_predictor.py`):**
    *   **Function**: `predict_sleep_times(user_id: str, schedule_agent_client) -> Optional[dict]`:
        *   Analyzes user's schedule from `ScheduleAgent` for free blocks at night.
        *   Considers past sleep patterns from `MemoryManagerAgent`.
        *   Returns `{'predicted_sleep_start': '...', 'predicted_wake_up': '...'}`.
        *   User can confirm/adjust via `ConversationAgent`.
    *   **Function**: `predict_meal_times(user_id: str, schedule_agent_client) -> List[dict]`:
        *   Identifies typical meal slots (breakfast, lunch, dinner) around scheduled events.
        *   Returns `[{'meal_type': 'lunch', 'predicted_time': '...'}]`.

4.  **Sub-Module Implementation:**

    *   **3.7.1. SleepModule (`sleep_module.py`):**
        *   **Function**: `log_sleep(user_id: str, sleep_data: SleepRecord)`: Stores sleep data (via MemoryManager/MCP).
        *   **Function**: `calculate_sleep_deficit(user_id: str, target_hours: float = 7.5) -> float`: Compares recent average sleep to target.
        *   **Function**: `suggest_sleep_recovery(deficit_hours: float) -> str`: Generates simple advice (e.g., "Try to get an extra hour of sleep tonight.").
        *   **Alerts**: If consistent deficit, generate a warning.

    *   **3.7.2. NutritionModule (`nutrition_module.py`):**
        *   **Function**: `log_meal(user_id: str, meal_data: MealRecord)`: Stores meal data.
            *   Integrate with food databases (e.g., Edamam, FatSecret API) or use LLM for nutritional estimation from description: "Estimate nutrients for 'chicken salad sandwich'".
        *   **Function**: `track_daily_nutrients(user_id: str, date_utc) -> dict`: Aggregates nutrients for the day.
        *   **Reminders**: "Time for your scheduled lunch." (via MCP notifications).

    *   **3.7.3. ActivityModule (`activity_module.py`):**
        *   **Function**: `log_activity(user_id: str, activity_data: ActivityRecord)`: Stores activity.
        *   **Function**: `suggest_exercise_routine(user_id: str, preferences: dict) -> str`: Based on user goals (e.g., "Suggest a 30-min home workout").
        *   **Alerts**: For prolonged inactivity if user opts-in.

    *   **3.7.4. WellnessModule (`wellness_module.py`):**
        *   **Function**: `log_wellness_checkin(user_id: str, wellness_data: WellnessLog)`: Stores stress/mood.
        *   **Function**: `analyze_stress_patterns(user_id: str, period_days: int = 7) -> Optional[str]`:
            *   Looks for correlations between high stress and schedule density, sleep quality.
            *   Uses simple heuristics or basic pattern detection (OpenAI SDK for more advanced analysis).
        *   **Function**: `recommend_recovery_routine(stress_level: int, available_time_minutes: int) -> str`: "High stress detected. Try a 10-minute mindfulness exercise."

5.  **Overwork Detection & Alerts (`overwork_analyzer.py`):**
    *   **Function**: `check_overwork(user_id: str, schedule_data: list, task_data: list, recent_activity: list) -> Optional[str]`:
        *   Analyzes calendar density, number of high-priority tasks, lack of breaks, reported stress.
        *   If potential overwork, generate an alert: "You have a very packed schedule and several upcoming deadlines. Remember to take breaks."

6.  **Weekly Health Summary (`health_reporter.py`):**
    *   **Function**: `generate_weekly_summary(user_id: str) -> str`:
        *   Aggregates sleep, activity, nutrition (if logged) for the past week.
        *   Presents trends and simple insights (e.g., "You averaged 7 hours of sleep this week, slightly below your target.").

7.  **OpenAI SDK Integration:**
    *   Nutrient estimation from food descriptions.
    *   Generating personalized suggestions for sleep, exercise, stress recovery.
    *   Analyzing free-text wellness logs for sentiment/key themes.
    *   Pattern recognition in health data for more advanced insights (future scope).

8.  **MCP Integration Points:**
    *   **Collect Data from Health Trackers (via MCP's potential aggregation services):**
        *   `GET /mcp/healthdata/{user_id}?source=garmin&type=sleep&since=...`
    *   **Store User Health Profile & Logs (if MCP provides secure data storage for health info):**
        *   `POST /mcp/healthlogs/{user_id}`
        *   Request: `{'type': 'sleep'/'meal'/..., 'data': {...}}`
    *   **Send Alerts & Reminders (via MCP Notification service):**
        *   `POST /mcp/notifications`
        *   Request: `{'user_id': '...', 'type': 'health_alert', 'message': '...'}`

*(Further agent details will be added below this section.)*
