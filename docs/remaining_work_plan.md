# Floe AI Assistant - Remaining Work Plan

## 1. Introduction

*   This document outlines the remaining work for the Floe AI Assistant. It is based on the current project status as documented in `docs/project_status_summary.md`, the overall project goals, and the detailed plans specified in `docs/implementation_plan.md` and `docs/product-overview.md`. The aim is to provide a clear roadmap for completing the subsequent phases of development.

## 2. Immediate Next Tasks

*   **Complete `TaskAgent` Implementation:**
    *   Finalize `task_agent/task_core.py`: This includes defining `TaskItem` Pydantic models, implementing CRUD (Create, Read, Update, Delete) operations for tasks, developing basic logic for task priority, and including a placeholder for future reminder functionality. This aligns with `docs/implementation_plan.md` (Section 3.5.2) and current status in `docs/project_status_summary.md`.
    *   Implement `task_agent/task_calendar_linker.py`: This module is responsible for calendar blocking integration, allowing tasks to be linked with `ScheduleAgent` for time allocation, as detailed in `docs/implementation_plan.md` (Section 3.5.3).
    *   Develop unit tests: Create comprehensive unit tests for all functionalities within `task_core.py` and `task_calendar_linker.py`.
    *   Conduct integration testing: Perform integration tests for `TaskAgent` to ensure it works correctly with `ScheduleAgent` (for calendar linking) and `MemoryManagerAgent` (for storing and retrieving task-related data).

## 3. Implementation of Remaining Agents

The following agents are key to expanding Floe's capabilities. Their implementation will follow the specifications in `docs/implementation_plan.md`.

### 3.1. ConversationAgent
    *   **Goal**: Implement core functionalities for natural language interaction, enabling smooth and context-aware user dialogue.
    *   **Modules to Implement (from `implementation_plan.md` Section 3.3):**
        *   `input_handler.py`: Focus initially on robust processing of text-based input. Voice input capabilities will be considered as a future enhancement.
        *   `dialogue_manager.py`: Develop logic for dialogue state tracking (maintaining context across turns), implementing clarification strategies when user input is ambiguous, handling task interruptions and re-engagement, and generating natural language responses.
    *   **Key Integrations**: `MemoryManagerAgent` (to store and retrieve dialogue history and context), `OrchestratorAgent` (to obtain intent and entity extractions from user queries).
    *   **Testing**: Implement unit tests for `input_handler.py` and `dialogue_manager.py`. Conduct integration tests to verify interactions with `OrchestratorAgent` and `MemoryManagerAgent`.

### 3.2. InboxAgent
    *   **Goal**: Implement functionalities for analyzing emails and other external messages, extracting relevant information, and routing it appropriately.
    *   **Modules to Implement (from `implementation_plan.md` Section 3.6):**
        *   `email_connectors.py`: Build connectors for Gmail and Outlook, ensuring secure management of OAuth tokens for authentication.
        *   `email_processor.py`: Implement LLM-based email summarization, develop capabilities to extract intents and entities (such as schedule proposals, task assignments, attachments), and route this information to the appropriate agents (e.g., `ScheduleAgent`, `TaskAgent`).
    *   **Key Integrations**: `ScheduleAgent` (for schedule proposals), `TaskAgent` (for identified tasks), `MemoryManagerAgent` (for archiving important information/attachments), and MCP (for connecting to email services and potentially routing extracted data).
    *   **Testing**: Create unit tests for connector logic and email processing functions. Perform integration tests with live email services (using dedicated test accounts) and with the agents receiving data from `InboxAgent`.

### 3.3. HealthAgent (Roadmap v1.1)
    *   **Goal**: Implement a suite of functionalities for automating personal health management, covering aspects like sleep, nutrition, activity, and general wellness. This aligns with Roadmap v1.1 from `docs/product-overview.md`.
    *   **Core Modules to Implement (from `implementation_plan.md` Section 3.7):**
        *   `health_models.py`: Define Pydantic models for various health records (sleep, meals, activities, wellness logs).
        *   `wearable_connectors.py`: Develop connectors for popular wearable devices/health platforms (potentially via MCP or direct API integration, ensuring secure OAuth handling).
        *   `health_predictor.py`: Implement logic to predict optimal sleep and meal times by analyzing user schedules and past patterns.
        *   `overwork_analyzer.py`: Create functionality to detect potential overwork by correlating schedule density, task load, and reported wellness data.
        *   `health_reporter.py`: Develop tools to generate weekly health summaries and insights.
    *   **Sub-Modules to Implement (from `implementation_plan.md` Section 3.7, referencing `work_plan.md`):**
        *   `sleep_module.py` (`SleepModule`): Implement sleep logging, calculation of sleep deficit, and generation of recovery suggestions.
        *   `nutrition_module.py` (`NutritionModule`): Enable meal logging, nutrient tracking (potentially using LLMs for estimation from descriptions), and meal reminders.
        *   `activity_module.py` (`ActivityModule`): Implement activity logging and provide suggestions for exercise routines.
        *   `wellness_module.py` (`WellnessModule`): Allow logging of stress/mood, analyze patterns, and recommend recovery routines.
    *   **Key Integrations**: `ScheduleAgent` (for contextual data for predictions), `MemoryManagerAgent` (for storing health logs and user preferences), MCP (for collecting data from health trackers and dispatching health-related notifications).
    *   **Testing**: Develop unit tests for all individual modules. Conduct integration tests using emulators for wearable devices or pre-defined test data sets.

### 3.4. InsightAgent (Roadmap v1.2)
    *   **Goal**: Implement functionalities for analyzing user behavior patterns across various domains, generating reports, and providing actionable insights to enhance productivity and wellbeing. This aligns with Roadmap v1.2 from `docs/product-overview.md`.
    *   **Modules to Implement (from `implementation_plan.md` Section 3.8):**
        *   `insight_generator.py`: This module will be responsible for generating daily and weekly insight reports, offering personalized routine recommendations, creating data for trend visualization (e.g., in JSON format for client-side rendering), tracking progress towards user-defined goals, and comparing metrics over different time periods.
    *   **Data Inputs**: The `InsightAgent` will consume aggregated data from `ScheduleAgent`, `TaskAgent`, `HealthAgent`, and `MemoryManagerAgent`.
    *   **Key Integrations**: MCP (for facilitating data aggregation from other agents and for sending notifications when new reports or insights are available).
    *   **Testing**: Create unit tests for insight generation logic. Perform integration tests focusing on the data aggregation pipeline and the accuracy of generated reports.

## 4. Further MCP Server Integration Development

*   **Goal**: Incrementally implement and thoroughly test the MCP server integration points as defined in `docs/implementation_plan.md` (Section 4) for each agent module as it undergoes development and refinement.
*   **Key Tasks**:
    *   Develop and/or configure MCP server endpoints that agents will communicate with (e.g., `POST /mcp/commands` for receiving user queries, `GET /mcp/memories/{user_id}/search` for memory retrieval).
    *   Implement client-side logic within each agent to interact with MCP APIs (e.g., `POST /mcp/invoke_service` if an agent needs to trigger another service via MCP, `POST /mcp/notifications` for sending alerts or updates).
    *   Ensure that all defined secure authentication (e.g., OAuth 2.0, JWTs for agent-MCP) and authorization mechanisms are correctly implemented and enforced.
    *   Set up, configure, and test asynchronous communication channels, such as message queues (e.g., RabbitMQ, Kafka via MCP) or webhooks, particularly for agents like `InboxAgent` (for new email notifications) or for dispatching system-wide notifications.
    *   Rigorously verify error handling mechanisms and implement resilience patterns (e.g., retries, fallbacks) for agent-MCP communications.

## 5. Execution of Broader Testing Strategy

*   **Goal**: Systematically execute and expand upon the comprehensive testing strategy outlined in `docs/implementation_plan.md` (Section 5) across all stages of development.
*   **Key Tasks**:
    *   **Continue Unit Testing**: Maintain and enforce high unit test coverage (aiming for >80-90% for critical modules) for all new and modified code within each agent.
    *   **Develop Integration Tests**: As individual agents and their modules are completed, develop and execute integration tests focusing on:
        *   Direct Agent-to-Agent interactions (e.g., `OrchestratorAgent` to `ScheduleAgent`).
        *   Agent-to-MCP interactions (both agent-as-client and agent-as-server if MCP calls agent endpoints).
        *   Agent-to-External Service interactions (e.g., `ScheduleAgent` with Google/Microsoft Calendar APIs, `InboxAgent` with email provider APIs).
    *   **Develop End-to-End (E2E) Tests**: Design and implement E2E tests that simulate key user scenarios from initial input to final output, covering complete flows through multiple agents and the MCP.
    *   **CI/CD Pipeline Enhancement**: Continuously integrate all types of automated tests (Unit, Integration, E2E) into the CI/CD pipeline. Ensure automated execution on commits/merges and clear reporting of test outcomes.
    *   **Test Data Management**: Establish and refine robust procedures for generating, managing, isolating, and cleaning up test data to ensure reliable and repeatable testing.

## 6. Steps Towards Deployment

*   **Goal**: Progressively prepare for and execute the deployment of Floe components according to the strategy detailed in `docs/implementation_plan.md` (Section 6).
*   **Key Tasks**:
    *   **Containerization**: Ensure all agents and any MCP-related components (if managed as separate services) have well-defined, optimized `Dockerfile`s.
    *   **Local Orchestration**: Maintain and improve `docker-compose` configurations to facilitate easy local development and testing of the multi-agent system.
    *   **Kubernetes Preparation**:
        *   Develop, test, and refine Kubernetes manifests (Deployments, Services, ConfigMaps, Secrets, etc.) for each deployable agent/service.
        *   Evaluate and potentially implement Helm charts for packaging and managing the deployment of Floe components on Kubernetes.
    *   **Environment Setup**: Plan and configure the necessary Testing/Staging and Production Kubernetes environments, prioritizing cloud-native solutions (AWS EKS, Google GKE, Azure AKS) where feasible.
    *   **Monitoring & Logging**: As agents are deployed to these environments, implement and configure monitoring tools (e.g., Prometheus for metrics, Grafana for dashboards) and a centralized logging solution (e.g., ELK stack, Loki, or cloud provider equivalents).
    *   **Configuration & Secrets Management**: Standardize how application configuration is injected (via ConfigMaps) and how secrets (API keys, credentials) are securely managed (via Kubernetes Secrets or dedicated solutions like HashiCorp Vault) within Kubernetes.

## 7. Alignment with Product Roadmap

*   **v1.0 (MVP)**: The immediate priority is completing the `TaskAgent`. This involves finalizing `task_core.py` and `task_calendar_linker.py`, and ensuring robust functionality of the `OrchestratorAgent`, `MemoryManagerAgent`, and `ScheduleAgent`. This phase includes the necessary MCP integrations and thorough testing for these core components, as per `docs/product-overview.md`.
*   **v1.1 Health**: Once the MVP components are stable and validated, development will transition to the `HealthAgent`, focusing on its core modules and sub-modules (Sleep, Nutrition, Activity, Wellness) as outlined in `docs/product-overview.md`.
*   **v1.2 Insights**: Following the successful implementation and testing of the `HealthAgent`, the focus will shift to the `InsightAgent`, enabling trend analysis, report generation, and goal tracking, as per `docs/product-overview.md`.
*   The `ConversationAgent` and `InboxAgent` provide foundational interaction and information-gathering capabilities. Their development and integration should be prioritized to support the functionalities of all roadmap versions, and can be developed in parallel or incrementally as needed.

This plan will be a living document, subject to updates and refinements as development progresses, new challenges are identified, and further information becomes available.
