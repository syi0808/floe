# Floe AI Assistant

Floe is a natural language-based AI assistant built with a modular, agent-based architecture. It aims to help users manage schedules, tasks, communication, health, and provide insights through an intelligent and conversational interface.

This repository contains the core components of the Floe system, including the OrchestratorAgent responsible for routing user intents to specialized agents.

## Project Status

This project is currently under active development. The `OrchestratorAgent` and `IntentAnalyzer` are functional, and a basic `MemoryManagerAgent` has been implemented. An interactive command-line interface is available to demonstrate and test the current orchestration capabilities with mock agents.

## Setup Instructions

### Prerequisites

*   Python 3.10 or higher.
*   Access to an LLM API (e.g., OpenAI, Mistral) and the corresponding API key.

### Installation

1.  **Clone the repository:**
    ```bash
    git clone <repository-url>
    cd <repository-directory>
    ```

2.  **Create a virtual environment (recommended):**
    ```bash
    python -m venv venv
    source venv/bin/activate  # On Windows: venv\Scripts\activate
    ```

3.  **Install dependencies:**
    ```bash
    pip install -r requirements.txt
    ```
    This project relies on several key packages including `pydantic`, `langdetect`,
    `litellm`, and Google's API libraries (`google-api-python-client`,
    `google-auth-httplib2`, `google-auth-oauthlib`). These are all listed in
    `requirements.txt` and `pyproject.toml`.

### Configuration

1.  **Create a `.env` file:**
    In the root directory of the project, create a file named `.env`.

2.  **Configure LLM settings in `.env`:**
    You need to specify the model name that LiteLLM will use and provide the necessary API key.

    Example for using an OpenAI model:
    ```env
    LITELLM_MODEL_NAME=gpt-3.5-turbo
    OPENAI_API_KEY=your_openai_api_key_here
    ```

    Example for using a Mistral model (via Mistral AI API):
    ```env
    LITELLM_MODEL_NAME=mistral/mistral-small-latest
    MISTRAL_API_KEY=your_mistral_api_key_here
    ```

    Example for using a Claude model (via Anthropic API):
    ```env
    LITELLM_MODEL_NAME=claude-2
    ANTHROPIC_API_KEY=your_anthropic_api_key_here
    ```

    *   Replace `your_..._api_key_here` with your actual API key.
    *   LiteLLM supports many models. Refer to the [LiteLLM documentation](https://docs.litellm.ai/docs/providers) for more provider options and model names. Ensure the chosen `LITELLM_MODEL_NAME` corresponds to the API key you provide.

## How to Run

The primary entry point for demonstrating the current capabilities is the interactive orchestrator agent CLI.

1.  **Ensure your virtual environment is activated and your `.env` file is configured.**

2.  **Run the interactive CLI:**
    ```bash
    python -m orchestrator_agent.main
    ```

3.  **Interact with the agent:**
    You can type queries at the prompt. Examples:
    *   `Schedule a meeting with Jane for tomorrow at 2 PM about the project budget.`
    *   `Remind me to buy milk tomorrow`
    *   `How are you today?`

    Type `exit` or `quit` to end the interactive session.

## Project Structure (Overview)

*   `orchestrator_agent/`: Contains the core orchestration logic.
    *   `main.py`: Interactive CLI entry point.
    *   `intent_analyzer.py`: Handles intent extraction using LiteLLM.
    *   `orchestrator_core.py`: The `OrchestrationEngine` for routing requests.
    *   `base_agent.py`: Defines the `BaseAgent` abstract class for all agents.
*   `memory_manager_agent/`: Contains the memory management logic.
    *   `memory_manager.py`: Basic `MemoryManagerAgent` implementation.
*   `docs/`: Contains current design documents and plans. Older planning notes and work summaries live in `docs/archive/`.
*   `tests/`: Contains unit and integration tests.
*   `README.md`: This file.

## Next Steps (High-Level from Plan)

*   Further develop `MemoryManagerAgent` with actual memory storage and retrieval capabilities.
*   Implement concrete versions of specialized agents (e.g., `ScheduleAgent`, `TaskAgent`) inheriting from `BaseAgent`.
*   Integrate these specialized agents with the `OrchestrationEngine`.
*   Develop a more robust application entry point (e.g., a web API using FastAPI).
*   Expand test coverage.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.

Contributions and feedback are welcome!
