# Floe AI Assistant - Work Plan

This document outlines the development plan for the Floe AI assistant, a personalized AI service for managing schedules, tasks, communication, health, and providing insights.

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

## 3. Agent Implementation Strategy

Each agent will be developed as a Python module, leveraging OpenAI's Agents SDK for core AI functionalities. Communication with the MCP server will be crucial for data exchange and triggering actions.

## 4. Detailed Agent Breakdown

This section will detail the implementation plan for each agent.

### 4.1. OrchestratorAgent
- **Core Role**: User command analysis and routing.
- **Key Features**:
    - Intent analysis from natural language.
    - Sequential and parallel orchestration of sub-agents.
    - Integration with MemoryManagerAgent for contextual information.
    - Aggregation of responses from multiple agents.
- **OpenAI Agents SDK Usage**: For intent recognition and complex decision-making.
- **MCP Integration**: To receive user commands and dispatch tasks to other services if needed.

### 4.2. MemoryManagerAgent
- **Core Role**: Long-term and short-term memory management.
- **Key Features**:
    - Vectorization and storage of diverse memory types (conversations, schedules, tasks, etc.).
    - Semantic search for memory retrieval.
    - TTL and recency-based prioritization.
    - Automatic context injection for other agents.
- **OpenAI Agents SDK Usage**: For embedding generation and semantic search capabilities.
- **MCP Integration**: For persisting and retrieving memory data from a central data store.

### 4.3. ConversationAgent
- **Core Role**: Natural language interaction and context maintenance.
- **Key Features**:
    - Handling text/voice input.
    - Dialogue flow management and inference.
    - Clarification questions.
    - Task interruption and re-engagement.
    - Cached intent processing for quick responses.
- **OpenAI Agents SDK Usage**: For natural language understanding, dialogue state tracking.
- **MCP Integration**: For sending/receiving messages via various communication channels.

### 4.4. ScheduleAgent (SchedulerAgent)
- **Core Role**: Schedule creation, conflict detection, and time recommendation.
- **Key Features**:
    - Parsing schedule details from natural language.
    - Integration with Google/Microsoft Calendars.
    - Meeting time recommendations based on attendee availability.
    - Handling complex and recurring schedules.
    - Schedule summarization.
- **OpenAI Agents SDK Usage**: For natural language parsing of dates, times, and locations.
- **MCP Integration**: To sync with external calendar services and potentially with other users' Floe instances for group scheduling.

### 4.5. TaskAgent (TaskManagerAgent)
- **Core Role**: Task creation, structuring, prioritization, and reminders.
- **Key Features**:
    - Structuring tasks from natural language commands.
    - Extracting action items from texts (emails, meeting notes).
    - Priority calculation based on due dates and importance.
    - Calendar blocking integration.
    - Automated linking of tasks and schedules.
- **OpenAI Agents SDK Usage**: For NLP-based task extraction and understanding dependencies.
- **MCP Integration**: To store tasks and link with calendar entries.

### 4.6. InboxAgent
- **Core Role**: Analyzing and extracting information from emails, notifications, and external messages.
- **Key Features**:
    - Gmail/Outlook integration.
    - LLM-based email summarization.
    - Recognizing schedule proposals/requests and forwarding to ScheduleAgent.
    - Extracting tasks/attachments and forwarding to TaskAgent.
    - Archiving meeting invites/files to MemoryManagerAgent.
- **OpenAI Agents SDK Usage**: For email content analysis, summarization, and intent extraction.
- **MCP Integration**: To connect with email services and route extracted information.

### 4.7. HealthAgent
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

### 4.8. InsightAgent
- **Core Role**: Analyzing user behavior patterns and generating reports.
- **Key Features**:
    - Integrated analysis of logs from various agents (schedule, tasks, sleep, etc.).
    - Generation of productivity and health reports.
    - Personalized routine recommendations.
    - Behavioral improvement notifications and trend visualization.
- **OpenAI Agents SDK Usage**: For data analysis, trend identification, and generating actionable insights.
- **MCP Integration**: To access logged data from all other agents.

## 5. MCP Server Integration Plan

Effective integration with the MCP server is critical. This will involve:
- **API Definitions**: Clear API contracts for each agent to communicate with the MCP server and potentially with each other via the server.
- **Data Schemas**: Standardized data formats for requests and responses.
- **Authentication & Authorization**: Secure communication channels.
- **Asynchronous Communication**: Utilizing message queues for non-blocking operations where applicable (e.g., email processing by InboxAgent).

## 6. Testing Strategy

- **Unit Tests**: Each agent's modules and functions will be tested individually.
- **Integration Tests**: Testing the interaction between agents, and between agents and the MCP server/external services.
- **End-to-End Tests**: Simulating user scenarios to test the entire Floe system.
- **Continuous Integration/Continuous Deployment (CI/CD)**: Automating the testing and deployment process.

## 7. Deployment Strategy

- Agents could be deployed as microservices.
- Containerization (e.g., Docker) for portability and scalability.
- Cloud platform deployment (e.g., AWS, Google Cloud, Azure) for managing services.

## 8. Timeline and Milestones (Placeholder)

(This section will be filled in with estimated timelines and key deliverables for each phase of the project.)

---

This work plan provides a roadmap for the development of the Floe AI assistant. It will be updated as the project progresses.
