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
        *   **Consideration for Multi-Turn Orchestrations**: While `route_request` handles discrete commands, more complex, multi-turn processes (e.g., a prolonged planning dialogue) would typically be managed by the `ConversationAgent` maintaining the dialogue state and re-invoking the `OrchestratorAgent` as needed for specific actions. Alternatively, the `OrchestratorAgent` could store an interim orchestration state in `MemoryManagerAgent` if a single orchestrated flow needs to persist across multiple distinct user interactions or asynchronous events.
    *   **Response Aggregation**:
        *   Develop a strategy for combining responses if multiple agents are invoked (e.g., summarization, structured list).
        *   Define a standard `AgentResponse` format: `{'status': 'success'/'error', 'data': ..., 'message': ..., 'source_agent': 'OrchestratorAgent'}`.

3.  **OpenAI SDK Integration:**
    *   Leverage `openai.ChatCompletion` with function definitions for robust intent and entity extraction.
    *   Explore Agents SDK features for more complex conversational flows if needed, though primary routing might be rule-based post-intent-extraction. This could involve leveraging specific SDK classes or frameworks for advanced dialogue management, state tracking, or streamlined tool/agent invocation if offered by the SDK.

    ```python
    # Example: Intent and Entity Extraction
    import openai
    import os
    import json

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def extract_intent_and_entities(user_query: str):
        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo-0613", # Or other suitable model that supports functions
                messages=[{"role": "user", "content": user_query}],
                functions=[
                    {
                        "name": "extract_schedule_info",
                        "description": "Extracts information for scheduling an event.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string", "description": "The title of the event."},
                                "participants": {"type": "array", "items": {"type": "string"}, "description": "List of participants."},
                                "time": {"type": "string", "description": "Time of the event, e.g., '2 PM'."},
                                "date": {"type": "string", "description": "Date of the event, e.g., 'tomorrow', 'next Tuesday'."},
                                "description": {"type": "string", "description": "Brief description or agenda for the event."}
                            },
                            "required": ["title", "participants", "time", "date"]
                        }
                    },
                    {
                        "name": "create_task",
                        "description": "Creates a new task.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "task_description": {"type": "string", "description": "The description of the task."},
                                "due_date": {"type": "string", "description": "Optional due date for the task."},
                                "priority": {"type": "string", "description": "Optional priority for the task (e.g., high, medium, low)."}
                            },
                            "required": ["task_description"]
                        }
                    }
                    # Add other function definitions for different intents
                ],
                function_call="auto" # Let the model decide which function to call
            )
            message = response.choices[0].message
            if message.get("function_call"):
                function_name = message["function_call"]["name"]
                arguments = json.loads(message["function_call"]["arguments"])
                # This structured data can now be used by OrchestratorAgent
                # to route to the correct agent (e.g., ScheduleAgent, TaskAgent)
                # or to perform an action.
                return {"intent": function_name, "entities": arguments}
            else:
                # Handle cases where no function call was made (e.g., general conversation)
                return {"intent": "general_conversation", "response_text": message.content}
        except Exception as e:
            # print(f"Error in OpenAI call: {e}")
            return {"error": str(e)}

    # Example usage:
    # intent_data = extract_intent_and_entities("Schedule a meeting with Jane for tomorrow at 2 PM about the project budget.")
    # if intent_data and "intent" in intent_data:
    #    print(f"Intent: {intent_data['intent']}")
    #    print(f"Entities: {intent_data['entities']}")
    #
    # intent_data_task = extract_intent_and_entities("Remind me to buy milk tomorrow")
    # if intent_data_task and "intent" in intent_data_task:
    #    print(f"Intent: {intent_data_task['intent']}")
    #    print(f"Entities: {intent_data_task['entities']}")
    ```

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
    *   **Information Updates and Conflict Handling**: The system primarily relies on timestamps and the 'last write wins' principle for updates to memory items via `update_memory`. Recency, coupled with TTLs, helps ensure that the most current information is prioritized during retrieval. More sophisticated conflict reconciliation or versioning strategies could be implemented if specific use cases demonstrate a need beyond this.

2.  **Memory Retrieval Module (`memory_retriever.py`):**
    *   **Semantic Search**:
        *   **Function**: `search_memories(user_id: str, query_text: str, top_k: int = 5, filter_types: Optional[List[str]] = None) -> List[BaseMemoryModel]`:
            *   Generates embedding for `query_text`.
            *   Performs similarity search in the vector DB for the given `user_id`.
            *   Applies filters for `memory_types` (e.g., only 'ConversationMemory').
            *   Implements TTL logic: Exclude expired memories.
            *   Implements recency prioritization: Optionally boost scores of recent memories.
            *   **Data Scoping**: While primary data isolation is by `user_id`, the `filter_types` parameter also allows calling agents to request only those memory types strictly relevant to their operational domain, providing a secondary level of data scoping.
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

    ```python
    # Example: Generating Embeddings
    import openai
    import os

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def get_embedding(text: str, model: str = "text-embedding-ada-002") -> list[float] | None:
        try:
            response = openai.Embedding.create(
                input=[text.replace("\n", " ")], # Model performs best with single line of text
                model=model
            )
            return response.data[0].embedding
        except Exception as e:
            # print(f"Error in generating embedding: {e}")
            return None

    # Example usage:
    # query_embedding = get_embedding("User query about project status.")
    # if query_embedding:
    #    print(f"Generated embedding, first 5 dimensions: {query_embedding[:5]}")
    #
    # document_text = "The project is on track for delivery next quarter. Key milestones have been met."
    # document_embedding = get_embedding(document_text)
    # if document_embedding:
    #    print(f"Generated document embedding, first 5 dimensions: {document_embedding[:5]}")

    ```

5.  **MCP Integration Points:**
    *   **Persist/Retrieve Memory Data (if MCP provides a central data store):** These endpoints provide a full RESTful interface to memory items managed by the agent via MCP, complementing the internal CRUD functions.
        *   Endpoint: `POST /mcp/memories/{user_id}` (for adding a new memory item)
        *   Request Schema: `{ 'type': 'conversation'/'task'/..., 'data': {...}, 'ttl_seconds': Optional[int] }`
        *   Endpoint: `GET /mcp/memories/{user_id}/search` (for semantic search of memory items)
        *   Request Schema: `{ 'query_text': '...', 'top_k': 5, 'filter_types': ['task'] }`
        *   Response Schema: `List[{'id': '...', 'type': '...', 'data': {...}, 'score': 0.89}]`
        *   Endpoint: `GET /mcp/memories/{user_id}/{memory_id}` (to retrieve a specific memory item by its ID)
        *   Response Schema: `{'id': '...', 'type': '...', 'data': {...}}`
        *   Endpoint: `PUT /mcp/memories/{user_id}/{memory_id}` (to update an existing memory item by its ID)
        *   Request Schema: `{ 'data': {...}, 'ttl_seconds': Optional[int] }` // Specific fields to update
        *   Endpoint: `DELETE /mcp/memories/{user_id}/{memory_id}` (to delete a specific memory item by its ID)
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
    *   Agents SDK: Explore for more robust dialogue state tracking and turn-by-turn interaction models provided by the SDK. This could involve leveraging specific SDK classes or frameworks for advanced dialogue management, state tracking, or streamlined tool/agent invocation if offered by the SDK.

    ```python
    # Example: Generating a Contextual Response
    import openai
    import os

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def generate_contextual_reply(conversation_history: list[dict], user_message: str) -> str | None:
        # Ensure conversation history is in the correct format, e.g., list of {"role": "user/assistant", "content": "..."}
        messages = conversation_history + [{"role": "user", "content": user_message}]
        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo", # Or other suitable model
                messages=messages
            )
            return response.choices[0].message.content
        except Exception as e:
            # print(f"Error in generating contextual reply: {e}")
            return None

    # Example usage:
    # history = [
    #    {"role": "user", "content": "What's the weather like today?"},
    #    {"role": "assistant", "content": "It's sunny and warm in San Francisco."}
    # ]
    # current_user_message = "That's great! What about in London?"
    # reply = generate_contextual_reply(history, current_user_message)
    # if reply:
    #    print(f"Assistant's reply: {reply}")
    #    history.append({"role": "user", "content": current_user_message})
    #    history.append({"role": "assistant", "content": reply})
    ```

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
        *   `GoogleCalendarConnector(AbstractCalendarConnector)`: Implements methods using Google Calendar API (OAuth2 for authentication). Authentication with Google Calendar will utilize OAuth 2.0. Secure token management will adhere to the principles outlined in the 'MCP Server Integration Plan' (Section 4.3) or employ a dedicated secrets management solution.
        *   `MicrosoftCalendarConnector(AbstractCalendarConnector)`: Implements methods using Microsoft Graph API (OAuth2). Authentication with Microsoft Graph will utilize OAuth 2.0. Secure token management will adhere to the principles outlined in the 'MCP Server Integration Plan' (Section 4.3) or employ a dedicated secrets management solution.
    *   User credentials (OAuth tokens) should be securely managed as per the above notes.

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
    ```python
    # Example: Parsing Schedule Details with Function Calling
    import openai
    import os
    import json

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def parse_schedule_from_query(natural_language_query: str):
        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo-0613", # Model supporting functions
                messages=[{"role": "user", "content": natural_language_query}],
                functions=[
                    {
                        "name": "extract_schedule_info",
                        "description": "Extracts detailed information for scheduling an event from natural language.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "event_title": {"type": "string", "description": "The title or subject of the event."},
                                "participants": {"type": "array", "items": {"type": "string"}, "description": "List of participant names or email addresses."},
                                "date_expression": {"type": "string", "description": "The date of the event (e.g., 'next Friday', 'August 15th', 'tomorrow')."},
                                "time_expression": {"type": "string", "description": "The time of the event (e.g., '3 PM', 'morning', 'evening')."},
                                "duration_minutes": {"type": "integer", "description": "Optional duration of the event in minutes."},
                                "location": {"type": "string", "description": "Optional location of the event."},
                                "recurrence_rule": {"type": "string", "description": "Optional recurrence rule (e.g., 'every week', 'monthly on the 1st')."}
                            },
                            "required": ["event_title", "date_expression", "time_expression"]
                        }
                    }
                ],
                function_call={"name": "extract_schedule_info"} # Force calling this function
            )
            message = response.choices[0].message
            if message.get("function_call"):
                arguments = json.loads(message["function_call"]["arguments"])
                # These arguments can then be processed by ScheduleAgent, including
                # normalizing dates/times using libraries like dateparser,
                # resolving participant contacts, etc.
                return arguments
            return None
        except Exception as e:
            # print(f"Error in parsing schedule details: {e}")
            return None

    # Example usage:
    # query = "Can we schedule a meeting with John and Alice for next Monday at 10am to discuss the Q3 roadmap? It should be about 1 hour."
    # schedule_details = parse_schedule_from_query(query)
    # if schedule_details:
    #    print(f"Parsed schedule details: {schedule_details}")
    #    # Further processing:
    #    # - Normalize date_expression and time_expression to actual datetime objects.
    #    # - Resolve participant names to user_ids or email addresses.
    #    # - Handle recurrence_rule.
    ```

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
    ```python
    # Example: Extracting Task Details with Function Calling
    import openai
    import os
    import json

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def extract_task_details(natural_language_query: str):
        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo-0613", # Model supporting functions
                messages=[{"role": "user", "content": natural_language_query}],
                functions=[
                    {
                        "name": "create_task_from_details",
                        "description": "Extracts task details like description, due date, and priority from natural language.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "description": {"type": "string", "description": "The full description of the task."},
                                "due_date": {"type": "string", "description": "Optional due date (e.g., 'tomorrow', 'end of week', 'July 20th')."},
                                "priority": {"type": "string", "enum": ["high", "medium", "low"], "description": "Optional task priority."},
                                "project": {"type": "string", "description": "Optional project or category for the task."}
                            },
                            "required": ["description"]
                        }
                    }
                ],
                function_call={"name": "create_task_from_details"}
            )
            message = response.choices[0].message
            if message.get("function_call"):
                arguments = json.loads(message["function_call"]["arguments"])
                # These arguments can be used by TaskAgent to create a new task.
                # Due dates would need further parsing/normalization.
                return arguments
            return None
        except Exception as e:
            # print(f"Error in extracting task details: {e}")
            return None

    # Example usage for creating a task:
    # task_query = "Add 'Finish the report for Project Alpha' to my tasks, it's due next Friday and is high priority."
    # task_details = extract_task_details(task_query)
    # if task_details:
    #    print(f"Parsed task details: {task_details}")

    # Example for action item extraction from a larger text:
    # action_item_query = "From the meeting notes: 'Sarah to draft the proposal by Monday. Team to review the budget next week.' Can you extract the action items?"
    # (A different function definition optimized for action item extraction might be needed here,
    # focusing on identifying assignees, actions, and deadlines from sentences.)
    # For instance:
    # {
    #    "name": "extract_action_items",
    #    "description": "Extracts one or more action items from a given text, identifying the action, assignee, and deadline.",
    #    "parameters": {
    #        "type": "object",
    #        "properties": {
    #            "action_items": {
    #                "type": "array",
    #                "items": {
    #                    "type": "object",
    #                    "properties": {
    #                        "action": {"type": "string"},
    #                        "assignee": {"type": "string"},
    #                        "deadline": {"type": "string"}
    #                    },
    #                    "required": ["action"]
    #                }
    #            }
    #        }
    #    }
    # }
    # action_items = extract_task_details(action_item_query) # Assuming the function is adapted
    # if action_items:
    #    print(f"Extracted action items: {action_items}")
    ```

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
        *   `GmailConnector(AbstractEmailConnector)`: Uses Gmail API (OAuth2). Authentication with Gmail API will utilize OAuth 2.0. Secure token management will adhere to the principles outlined in the 'MCP Server Integration Plan' (Section 4.3) or employ a dedicated secrets management solution.
        *   `OutlookConnector(AbstractEmailConnector)`: Uses Microsoft Graph API (OAuth2). Authentication with Microsoft Graph API will utilize OAuth 2.0. Secure token management will adhere to the principles outlined in the 'MCP Server Integration Plan' (Section 4.3) or employ a dedicated secrets management solution.
    *   Handles new email detection (e.g., polling, push notifications if supported by API and MCP).
    *   User credentials (OAuth tokens) managed securely as per the above notes.

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

    ```python
    # Example: Email Summarization
    import openai
    import os

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def summarize_email_text(email_body: str, max_tokens: int = 150) -> str | None:
        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo",
                messages=[
                    {"role": "system", "content": "Summarize the following email text concisely, focusing on key information and action items."},
                    {"role": "user", "content": email_body}
                ],
                max_tokens=max_tokens,
                temperature=0.5 # Adjust for more factual summary
            )
            return response.choices[0].message.content
        except Exception as e:
            # print(f"Error in summarizing email: {e}")
            return None

    # Example Usage:
    # long_email_text = """Subject: Project Update & Next Steps
    # Hi team, Just a quick update on Project Phoenix. We've successfully completed Phase 1 milestones.
    # The client feedback was largely positive, though they raised a concern about the timeline for Phase 2.
    # John, can you please look into optimizing the deployment script? Sarah, please prepare the presentation
    # for next week's review. We need to finalize the budget by EOD Friday.
    # Thanks, Alex"""
    # summary = summarize_email_text(long_email_text)
    # if summary:
    #    print(f"Email Summary: {summary}")

    # Example: Extracting Actions/Intents from Email
    # This would use a function calling approach similar to OrchestratorAgent (3.1.3)
    # or TaskAgent (3.5.5). The functions defined would be specific to email contexts.
    # For example:
    # functions = [
    #    {
    #        "name": "extract_schedule_proposal_from_email",
    #        "description": "Identifies and extracts details of a schedule proposal from an email.",
    #        "parameters": {
    #            "type": "object",
    #            "properties": {
    #                "proposed_times": {"type": "array", "items": {"type": "string"}, "description": "Suggested dates/times for the meeting."},
    #                "subject": {"type": "string", "description": "Subject of the proposed meeting."},
    #                "context": {"type": "string", "description": "Brief context of the proposal."}
    #            },
    #            "required": ["proposed_times"]
    #        }
    #    },
    #    {
    #        "name": "extract_task_from_email",
    #        "description": "Identifies and extracts tasks assigned or requested in an email.",
    #        "parameters": { /* Similar to TaskAgent's create_task_from_details */ }
    #    },
    #    {
    #        "name": "archive_document_info_from_email",
    #        "description": "Identifies information about documents or attachments to be archived.",
    #        "parameters": { /* Schema for document name, type, source link if any */ }
    #    }
    # ]
    # To use this, one would call openai.ChatCompletion.create with the email content as a message
    # and these function definitions.
    # e.g., email_content = "User: Hi, can we meet next Tuesday or Wednesday? John mentioned he wants to discuss the report."
    # extracted_info = process_email_with_functions(email_content, functions_definition)
    # print(extracted_info)
    ```

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
        *   `GarminConnector`, `FitbitConnector`, etc., using their respective APIs (OAuth). Authentication with these services will utilize OAuth 2.0. Secure token management will adhere to the principles outlined in the 'MCP Server Integration Plan' (Section 4.3) or employ a dedicated secrets management solution.
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

    ```python
    # Example: Generating Personalized Health Suggestion
    import openai
    import os

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def get_personalized_health_suggestion(user_health_data: dict) -> str | None:
        # user_health_data might contain fields like:
        # {"stress_level": "high", "avg_sleep_hours": 5.5, "activity_level": "low",
        #  "reported_mood": "anxious", "preferences": ["walking", "meditation"]}
        prompt = f"Based on the following user health data, provide a concise, actionable, and personalized health suggestion: {str(user_health_data)}"
        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo",
                messages=[
                    {"role": "system", "content": "You are a supportive AI health assistant providing personalized, actionable advice."},
                    {"role": "user", "content": prompt}
                ],
                temperature=0.7, # Allow some creativity in suggestions
                max_tokens=150
            )
            return response.choices[0].message.content
        except Exception as e:
            # print(f"Error in generating health suggestion: {e}")
            return None

    # Example usage:
    # health_data = {"stress_level": "high", "avg_sleep_hours": 5.5, "activity_level": "low", "preferences": ["yoga", "reading"]}
    # suggestion = get_personalized_health_suggestion(health_data)
    # if suggestion:
    #    print(f"Health Suggestion: {suggestion}")

    # Example: Nutrient Estimation from Food Description (using function calling)
    import json
    def estimate_nutrients_for_food(food_description: str) -> dict | None:
        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo-0613", # Model supporting functions
                messages=[{"role": "user", "content": f"Estimate the nutrients for: {food_description}"}],
                functions=[
                   {
                       "name": "estimate_food_nutrients",
                       "description": "Estimates calories, protein, carbohydrates, and fat for a given food item or meal description.",
                       "parameters": {
                           "type": "object",
                           "properties": {
                               "food_item_description": {"type": "string", "description": "The description of the food item or meal."},
                               "calories": {"type": "integer", "description": "Estimated calories in kcal."},
                               "protein_grams": {"type": "integer", "description": "Estimated protein in grams."},
                               "carbs_grams": {"type": "integer", "description": "Estimated carbohydrates in grams."},
                               "fat_grams": {"type": "integer", "description": "Estimated fat in grams."}
                           },
                           "required": ["food_item_description", "calories", "protein_grams", "carbs_grams", "fat_grams"]
                       }
                   }
                ],
                function_call={"name": "estimate_food_nutrients"}
            )
            message = response.choices[0].message
            if message.get("function_call"):
                arguments = json.loads(message["function_call"]["arguments"])
                return arguments
            return None # Or handle cases where function wasn't called as expected
        except Exception as e:
            # print(f"Error in nutrient estimation: {e}")
            return None

    # Example usage:
    # food_desc = "A bowl of oatmeal with fresh berries, a tablespoon of chia seeds, and a drizzle of honey."
    # nutrients = estimate_nutrients_for_food(food_desc)
    # if nutrients:
    #    print(f"Estimated Nutrients for '{nutrients.get('food_item_description')}':")
    #    print(f"  Calories: {nutrients.get('calories')} kcal")
    #    print(f"  Protein: {nutrients.get('protein_grams')}g")
    #    print(f"  Carbs: {nutrients.get('carbs_grams')}g")
    #    print(f"  Fat: {nutrients.get('fat_grams')}g")
    ```

8.  **MCP Integration Points:**
    *   **Collect Data from Health Trackers (via MCP's potential aggregation services):**
        *   `GET /mcp/healthdata/{user_id}?source=garmin&type=sleep&since=...`
    *   **Store User Health Profile & Logs (if MCP provides secure data storage for health info):**
        *   `POST /mcp/healthlogs/{user_id}`
        *   Request: `{'type': 'sleep'/'meal'/..., 'data': {...}}`
    *   **Send Alerts & Reminders (via MCP Notification service):**
        *   `POST /mcp/notifications`
        *   Request: `{'user_id': '...', 'type': 'health_alert', 'message': '...'}`

### 3.8. InsightAgent

**From work_plan.md & insight-agent.md:**
- **Core Role**: Analyzing user behavior patterns across various domains (schedule, tasks, health) and generating reports with actionable insights to help the user understand and improve productivity and wellbeing.
- **Key Features**:
    - Integrated analysis of logs and aggregated data from ScheduleAgent, TaskAgent, HealthAgent, and MemoryManagerAgent.
    - Generation of productivity and health reports (e.g., Daily Brief, Weekly Review).
    - Personalized routine recommendations based on identified patterns.
    - Behavioral improvement notifications and trend visualization (JSON spec for client-side rendering).
    - Comparison of metrics over different periods.
    - Tracking progress towards user-defined goals.
- **OpenAI Agents SDK Usage**: For data analysis, complex pattern recognition, trend identification in user data, and generating personalized, actionable insights and report narratives.
    ```python
    # Example: Generating Report Narrative Snippet for InsightAgent
    import openai
    import os

    # Ensure API key is set, e.g., openai.api_key = os.environ.get("OPENAI_API_KEY")

    def generate_productivity_insight_narrative(analyzed_data: dict) -> str | None:
        # analyzed_data could contain summaries like:
        # {"tasks_completed_this_week": 15, "tasks_pending": 5, "avg_focus_hours_daily": 2.5,
        #  "meetings_attended": 7, "most_productive_day": "Wednesday"}

        prompt_parts = ["User's productivity data for the week:"]
        if 'tasks_completed_this_week' in analyzed_data:
            prompt_parts.append(f"- Completed {analyzed_data['tasks_completed_this_week']} tasks.")
        if 'tasks_pending' in analyzed_data:
            prompt_parts.append(f"- {analyzed_data['tasks_pending']} tasks are pending.")
        if 'avg_focus_hours_daily' in analyzed_data:
            prompt_parts.append(f"- Averaged {analyzed_data['avg_focus_hours_daily']} focus hours daily.")
        if 'meetings_attended' in analyzed_data:
            prompt_parts.append(f"- Attended {analyzed_data['meetings_attended']} meetings.")
        if 'most_productive_day' in analyzed_data:
            prompt_parts.append(f"- Most productive day was {analyzed_data['most_productive_day']}.")

        prompt_parts.append("\nGenerate a brief (2-3 sentences) narrative insight based on this data, highlighting achievements or areas for potential focus.")
        prompt = "\n".join(prompt_parts)

        try:
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo",
                messages=[
                    {"role": "system", "content": "You are an AI assistant that generates concise, encouraging, and actionable productivity insights for user reports."},
                    {"role": "user", "content": prompt}
                ],
                max_tokens=100,
                temperature=0.6
            )
            return response.choices[0].message.content
        except Exception as e:
            # print(f"Error in generating report narrative: {e}")
            return None

    # Example usage:
    # weekly_data_summary = {
    #    "tasks_completed_this_week": 15,
    #    "tasks_pending": 3,
    #    "avg_focus_hours_daily": 3.0,
    #    "meetings_attended": 5,
    #    "most_productive_day": "Tuesday"
    # }
    # narrative = generate_productivity_insight_narrative(weekly_data_summary)
    # if narrative:
    #    print(f"Productivity Insight: {narrative}")
    ```
- **MCP Integration Points**:
    - **Access Logged Data**:
        - Endpoint: `GET /mcp/data_aggregates/{user_id}?sources=schedule,tasks,health,memory&period=weekly&date=YYYY-MM-DD`  # Changed data_aggregate to data_aggregates
        - Request Schema: (Implicitly defined by query params)
        - Response Schema: Aggregated data structures from various agents.
    - **Store/Retrieve User Goals (if applicable):**
        - Endpoint: `POST /mcp/goals/{user_id}`
        - Request Schema: `{'goal_id': '...', 'description': '...', 'metrics': [...]}`
        - Endpoint: `GET /mcp/goals/{user_id}/{goal_id}`
    - **Send Report Notifications (via MCP Notification service):**
        - Endpoint: `POST /mcp/notifications`
        - Request Schema: `{'user_id': '...', 'type': 'insight_report_ready', 'message': 'Your weekly productivity report is available.', 'report_reference': '...'}`
- **Data Inputs**:
    - Aggregated event/task statistics from `ScheduleAgent` and `TaskAgent` (e.g., number of events, task completion rates).
    - Aggregated counts and summaries from `MemoryManagerAgent` (e.g., tasks completed vs. postponed, memory usage patterns).
    - Key Performance Indicators (KPIs) from `HealthAgent` (e.g., average sleep score, activity load, stress patterns).
- **Skills/Example Call (adapted for implementation plan context):**
    - **Core Skill Implementation (`insight_generator.py`):**
        - **Function**: `generate_report(user_id: str, period: str, focus: Optional[str] = None) -> dict`:
            - `period`: e.g., "daily", "weekly_YYYY-WW", "monthly_YYYY-MM".
            - `focus`: e.g., "productivity", "wellbeing", "sleep".
            - Fetches necessary data from MCP or directly from other agents (via clients).
            - Analyzes data to identify trends, achievements, areas for improvement.
            - Generates report content (markdown for text, JSON for charts).
            - Example: `generate_report(user_id="user123", period="weekly_2025-W20", focus="productivity")`
            - Returns: `{'report_markdown': '...', 'chart_json_spec': {...}}`
        - **Function**: `compare_metric_over_periods(user_id: str, metric: str, period1_start: str, period1_end: str, period2_start: str, period2_end: str) -> dict`:
            - `metric`: e.g., "tasks_completed", "avg_sleep_score".
            - Fetches metric data for the two periods.
            - Calculates difference and provides context.
            - Returns: `{'metric': '...', 'period1_value': ..., 'period2_value': ..., 'diff': ..., 'interpretation': '...'}`
        - **Function**: `track_goal_progress(user_id: str, goal_id: str) -> dict`:
            - Retrieves goal definition (from MCP or internal storage).
            - Fetches relevant data to assess progress against goal metrics.
            - Returns: `{'goal_id': '...', 'status': 'on_track'/'at_risk'/'achieved', 'progress_percent': 75, 'summary': '...'}`
    - **Report Templates**:
        - `Daily Brief`: Agenda summary, top priority tasks, quick wellness tip.
        - `Weekly Review`: Highlights successes, identifies areas where tasks slipped, suggests focus for next week.
    - **Visualization Output**:
        - Methods like `generate_report` will return a JSON structure compatible with a client-side charting library (e.g., Recharts), defining chart types, data, and labels. Example: `{'chart_type': 'bar', 'data_keys': ['completed', 'pending'], 'dataset': [{'date': 'Mon', 'completed': 5, 'pending': 2}, ...]}`.

## 4. MCP Server Integration Plan

Effective and robust integration with the Master Control Program (MCP) server is paramount for Floe's functionality. This plan outlines the key aspects of this integration, ensuring seamless data exchange, service invocation, and overall system coherence. The MCP server acts as a central hub for many operations, including routing, data persistence, and inter-agent communication facilitation where direct agent-to-agent calls are not suitable.

**4.1. API Definitions**
*   **Principle**: APIs exposed by agents (for MCP or other agents to call) and APIs exposed by MCP (for agents to call) will adhere to RESTful principles where applicable.
*   **Specification**: OpenAPI Specification (formerly Swagger) will be used to define all API contracts. This ensures clear, language-agnostic definitions that can be used for documentation generation, client SDK creation, and automated testing.
*   **Versioning**: APIs will be versioned (e.g., `/mcp/v1/tasks`, `/agent/schedule/v1/events`) to allow for evolution without breaking existing consumers. A clear deprecation policy for older API versions will be established.
*   **Key Endpoints (Examples from Agent Details):**
    *   MCP to Agents: `POST /mcp/commands`, `GET /mcp/memories/{user_id}/search`, `POST /mcp/conversation/{user_id}/message`
    *   Agents to MCP: `POST /mcp/invoke_service`, `POST /mcp/memories/{user_id}`, `POST /mcp/send_reply`, `POST /mcp/notifications`
    *   Agent-to-Agent (potentially proxied via MCP or direct): Defined per agent interaction needs (e.g., Orchestrator calling ScheduleAgent).

**4.2. Data Schemas**
*   **Standardization**: Pydantic models will be the primary method for defining data structures within Python-based agents. These models will be automatically translatable to JSON Schema for API validation and documentation.
*   **Format**: JSON will be the standard data interchange format for all API requests and responses.
*   **Validation**: MCP and individual agents will perform strict validation of incoming data against the defined schemas. Consistent error responses will be provided for schema violations.

**4.3. Authentication & Authorization**
*   **Agent-to-MCP**: Secure tokens (e.g., OAuth 2.0 client credentials flow or signed JWTs) will be used for agents to authenticate with MCP APIs. Each agent instance may receive unique credentials.
*   **User-to-MCP (via Client Applications)**: User authentication will be handled by MCP, likely using OAuth 2.0 authorization code flow or OpenID Connect. Agents performing actions on behalf of a user will receive context about the user, but not raw user credentials.
*   **Permissions**: Role-Based Access Control (RBAC) or fine-grained permissions will be defined within MCP to control what actions agents can perform and what data they can access, especially concerning user-specific information.

**4.4. Asynchronous Communication**
*   **Message Queues**: For tasks that are long-running or can be decoupled, message queues (e.g., RabbitMQ, Apache Kafka, or cloud-native solutions like AWS SQS/Google Pub/Sub if MCP supports them) will be utilized. Examples:
    *   `InboxAgent` processing new emails.
    *   `HealthAgent` processing data from wearables.
    *   Dispatching notifications via MCP.
*   **Webhooks**: MCP might use webhooks to notify agents of certain events (e.g., new user data available). Agents needing to expose webhook endpoints must do so securely.
*   **Callbacks**: For some interactions, asynchronous callbacks might be registered with the MCP.

**4.5. Error Handling**
*   **Standard HTTP Codes**: APIs will use standard HTTP status codes to indicate success, client errors, or server errors.
*   **Error Response Body**: Error responses will have a consistent JSON structure, e.g., `{'error_code': '...', 'message': '...', 'details': {...}}`.
*   **Resilience**: Agents should implement retries with exponential backoff for transient errors when communicating with MCP. Circuit breaker patterns might be employed for services prone to unresponsiveness.

**4.6. API Gateway**
*   **Consideration**: The use of an API Gateway (e.g., AWS API Gateway, Apigee, Kong) in front of MCP and potentially agent services should be evaluated.
*   **Benefits**: An API Gateway can provide request routing, rate limiting, unified authentication, logging, and caching, simplifying the interfaces of the backend services.

**4.7. Leveraging Existing MCP Infrastructure or Generic Services**

While the MCP defines specific logical capabilities and interfaces essential for Floe's operation, its implementation can be flexible. Where feasible, the MCP's functionalities can be mapped to or integrated with components of pre-existing enterprise systems or standard generic backend services. This approach promotes reuse, leverages established infrastructure, and can accelerate development.

*   **Data Persistence**:
    *   MCP's role in data storage for memories (MemoryManagerAgent), tasks (TaskAgent), user profiles, health logs (HealthAgent), etc., can be implemented by utilizing existing enterprise-grade databases rather than building a new persistence layer from scratch.
    *   These could include relational databases (e.g., PostgreSQL, MySQL, SQL Server), NoSQL databases (e.g., MongoDB, Cassandra), or document stores, depending on the data model and scalability requirements for each data type.
    *   The defined MCP API endpoints for data (e.g., `/mcp/memories/{user_id}`, `/mcp/tasks/{user_id}`) would then act as a standardized facade over these underlying enterprise data stores, ensuring a consistent interface for the agents.

*   **Asynchronous Communication & Message Queues**:
    *   MCP's requirements for asynchronous operations, such as notifications, background email processing by `InboxAgent`, or long-running tasks initiated by any agent, can integrate with established message bus or queueing systems.
    *   Examples include Apache Kafka, RabbitMQ, or cloud-provider specific services like Azure Service Bus, Google Cloud Pub/Sub, or AWS SQS/SNS.
    *   The MCP would define the message schemas (payload structures) for inter-agent or agent-MCP communication over these queues and orchestrate the producers and consumers (agents or MCP components).

*   **API Gateway Integration**:
    *   As mentioned in Section 4.6, if a corporate standard API Gateway (e.g., Apigee, Kong, AWS API Gateway, Azure API Management) is already in place, the MCP's APIs (and potentially individual agent APIs if exposed externally) would ideally be exposed and managed through it.
    *   This allows Floe to inherit existing policies for security (e.g., OAuth 2.0 enforcement, threat protection), rate limiting, traffic management, request/response transformation, and standardized logging/monitoring of API traffic.

*   **Authentication and Authorization**:
    *   MCP's user authentication and agent service authentication/authorization can integrate with existing enterprise Identity and Access Management (IAM) or Single Sign-On (SSO) solutions.
    *   This could involve using an existing OAuth 2.0 provider, OpenID Connect (OIDC) for user identity, or integrating with LDAP/Active Directory for identity verification. Agent service identities might be managed via service accounts or client credentials within such systems.
    *   This centralizes identity management and allows access policies to be managed consistently with other enterprise applications.

*   **Service Discovery**:
    *   While Kubernetes provides DNS-based service discovery, if the deployment environment utilizes a more extensive enterprise service discovery mechanism (e.g., HashiCorp Consul, CoreOS etcd, or cloud provider-specific registries like AWS Cloud Map or Azure Service Fabric Naming Service), Floe agents and MCP services can register themselves and discover dependencies through it.
    *   This can be particularly relevant in hybrid environments or where non-containerized legacy services need to interact with Floe.

*   **Logging and Monitoring Infrastructure**:
    *   Instead of setting up entirely new logging and monitoring stacks, agents and MCP components should be configured to push logs and metrics to existing centralized systems if available.
    *   This includes forwarding structured logs to platforms like Splunk, ELK Stack (Elasticsearch, Logstash, Kibana), or cloud provider solutions (AWS CloudWatch Logs, Google Cloud Logging, Azure Monitor Logs).
    *   Similarly, metrics (application-level, performance, and health metrics) can be exported to systems like Datadog, Dynatrace, Prometheus (if an enterprise instance exists), or cloud provider monitoring tools, allowing for unified operational visibility.

By adopting these integration strategies, Floe can become a well-integrated component of a broader enterprise IT landscape, focusing its unique value on the AI-driven agent functionalities while relying on robust, existing infrastructure for common backend needs.

## 5. Testing Strategy

A comprehensive testing strategy is crucial to ensure the reliability, correctness, and performance of the Floe AI assistant and its constituent agents. The strategy encompasses various levels of testing and aims for high automation.

**5.1. Unit Tests**
*   **Scope**: Each function and class method within an agent's modules will be tested in isolation. Business logic, parsers, and utility functions are key targets.
*   **Tools**:
    *   Python: `pytest` (preferred for its conciseness and rich plugin ecosystem) or `unittest`.
    *   Mocking: `unittest.mock` (for Python) to isolate dependencies (e.g., external API calls, database interactions, other agent clients).
*   **Coverage**: Aim for high code coverage (e.g., >80-90%) for critical modules.
*   **Execution**: Run automatically on every commit/push to version control.

**5.2. Integration Tests**
*   **Scope**: Verify interactions between components.
    *   **Agent-to-Agent**: Test direct communication paths between agents (e.g., `OrchestratorAgent` correctly calling `ScheduleAgent`). This involves using actual agent clients but may mock the internal logic of the called agent to focus on the interaction contract.
    *   **Agent-to-MCP**: Test the agent's ability to correctly consume MCP APIs and for MCP to correctly invoke agent APIs (if applicable). This will involve testing against a live (dev/test environment) MCP or a highly accurate mock of MCP.
    *   **Agent-to-External Services**: Test integration with external services like Google Calendar, email providers, or health data sources. These tests might be more limited in CI due to external dependencies and may rely on VCR/cassette testing or dedicated test accounts.
*   **Tools**: `pytest` can also be used for integration tests. HTTP client libraries (like `requests` or `httpx`) for API testing.
*   **Data**: Test data will need to be carefully managed, computationally generated using fixture libraries or pre-populated test databases.

**5.3. End-to-End (E2E) Tests**
*   **Scope**: Simulate complete user scenarios from input to the system (e.g., a natural language command) through all relevant agents and MCP interactions, to the final output or side effect.
*   **Examples**:
    *   "User schedules a meeting for tomorrow at 10 AM with John." -> Verify Orchestrator, ScheduleAgent, Calendar integration, and MemoryManagerAgent interactions.
    *   "User receives an email with an action item." -> Verify InboxAgent, TaskAgent, and notification flow via MCP.
*   **Tools**: API-driven E2E tests will be the primary focus, using scripting with `pytest` and HTTP clients. If a UI is eventually part of Floe directly, tools like Selenium or Playwright might be considered for UI testing.
*   **Environment**: E2E tests typically run in a dedicated, stable test environment that mirrors production as closely as possible.

**5.4. Test Data Management**
*   **Isolation**: Tests should be independent and manage their own test data to avoid interference.
*   **Generation**: Strategies for generating realistic test data (e.g., using libraries like Faker, or anonymized production samples where appropriate and secure).
*   **Cleanup**: Test environments should be reset or test data cleaned up after test runs to ensure repeatability.

**5.5. Continuous Integration/Continuous Deployment (CI/CD)**
*   **Automation**: All tests (unit, integration, and a subset of E2E) will be integrated into a CI/CD pipeline (e.g., GitHub Actions, Jenkins, GitLab CI).
*   **Gating**: Failed tests will prevent code merges or deployments to higher environments.
*   **Reporting**: Test results and coverage reports will be published and monitored.

**5.6. Performance Testing (Future Scope)**
*   **Objective**: Once core functionality is stable, performance tests will be designed to assess system responsiveness, throughput, and resource utilization under load.
*   **Tools**: Tools like Locust, k6, or JMeter could be used.

## 6. Deployment Strategy

The deployment strategy for Floe aims for scalability, maintainability, and resilience. A microservice-oriented architecture where agents are deployed independently is the target.

**6.1. Containerization**
*   **Technology**: Docker will be used to containerize each agent and any supporting services.
*   `Dockerfile`s will be optimized for small image sizes, security, and efficient builds.
*   **Local Development**: `docker-compose` will be used to orchestrate multi-container setups for local development and testing, simulating the interaction of various agents and MCP (if a local MCP version is available).

**6.2. Orchestration**
*   **Technology**: Kubernetes (K8s) is the preferred container orchestration platform for managing deployments, scaling, and service discovery in staging and production environments.
*   **Manifests**: Kubernetes manifests (YAML files defining Deployments, Services, ConfigMaps, Secrets, etc.) will be managed in version control. Helm charts may be used for packaging and managing K8s applications.
*   **Service Discovery**: K8s DNS will be used for service discovery between agents.

**6.3. Environment Strategy**
*   **Development**: Local Docker setups, potentially a shared dev Kubernetes cluster.
*   **Testing/Staging**: A dedicated Kubernetes cluster that mirrors production as closely as possible. Used for CI/CD automated testing and UAT.
*   **Production**: A robust, highly available Kubernetes cluster.

**6.4. Cloud vs. On-Premise**
*   **Cloud-Native Preferred**: Deployment on a major cloud provider (AWS, Google Cloud, Azure) is preferred to leverage managed Kubernetes services (EKS, GKE, AKS), scalable databases, message queues, and other infrastructure components.
*   **On-Premise Consideration**: If specific requirements dictate on-premise deployment, a self-managed Kubernetes cluster (e.g., using Kubeadm, Rancher) would be the approach. This increases operational overhead.

**6.5. Scalability**
*   **Horizontal Scaling**: Agents will be designed to be stateless where possible, allowing for horizontal scaling by increasing the number of container replicas in Kubernetes.
*   **Autoscaling**: Kubernetes Horizontal Pod Autoscaler (HPA) will be configured based on CPU/memory usage or custom metrics.
*   **Load Balancing**: K8s Services and Ingress controllers will manage load balancing across agent instances.

**6.6. Monitoring & Alerting**
*   **Metrics**: Prometheus will be used for collecting metrics from agents and K8s. Key metrics include request rates, error rates, latency, resource utilization. Agents may expose custom metrics.
*   **Dashboards**: Grafana will be used to visualize metrics and create operational dashboards.
*   **Alerting**: Alertmanager (part of Prometheus ecosystem) or cloud provider specific alerting tools will be configured for critical issues.
*   **Health Checks**: Kubernetes liveness and readiness probes will be implemented for each agent to ensure traffic is only routed to healthy instances.

**6.7. Logging**
*   **Centralized Logging**: Logs from all agents (stdout/stderr from containers) will be collected and aggregated into a centralized logging system (e.g., ELK Stack - Elasticsearch, Logstash, Kibana; or cloud solutions like AWS CloudWatch Logs, Google Cloud Logging).
*   **Structured Logging**: Logs should be in a structured format (e.g., JSON) to facilitate easier searching and analysis.
*   **Correlation IDs**: Implement correlation IDs that propagate through requests across multiple agents to aid in tracing and debugging.

**6.8. Configuration Management**
*   **Environment Variables**: Configuration will primarily be managed via environment variables, injected into containers by Kubernetes (using ConfigMaps and Secrets).
*   **Secrets Management**: Sensitive data (API keys, database passwords) will be stored in Kubernetes Secrets or a dedicated secrets manager (e.g., HashiCorp Vault, AWS Secrets Manager).
