# Orchestrator Agent

This agent is responsible for analyzing user commands, routing them to the appropriate specialized agents, and managing the overall workflow.

## Setup & Installation

1.  **Create a virtual environment** (recommended):
    ```bash
    python -m venv .venv
    source .venv/bin/activate  # On Windows use `.venv\Scripts\activate`
    ```

2.  **Install dependencies**:
    Make sure you have `uv` installed (`pip install uv`).
    ```bash
    uv pip install -r requirements.txt
    ```

3.  **Set up Environment Variables**:
    Create a `.env` file in the root of the project and add your OpenAI API key:
    ```env
    OPENAI_API_KEY="your_openai_api_key_here"
    ```
    The `intent_analyzer.py` is set up to use this key if provided, or you can pass it directly.
    The `main.py` in `orchestrator_agent` will demonstrate loading it from the .env file.
