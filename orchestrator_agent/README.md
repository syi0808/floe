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
    Create a `.env` file in the root of the project. The specific variables depend on the LLM provider you intend to use with LiteLLM.

    *   **For OpenAI (if still used via LiteLLM or for `openai-agents` parts):**
        ```env
        OPENAI_API_KEY="your_openai_api_key_here"
        ```
    *   **For Mistral API (via LiteLLM):**
        ```env
        MISTRAL_API_KEY="your_mistral_api_key_here"
        ```
    *   **General LiteLLM model configuration:**
        You'll also need to tell LiteLLM which model to use. This can be done by setting a `LITELLM_MODEL_NAME` in your `.env` file, which `intent_analyzer.py` can then pick up. Or, the model can be specified directly in the code. For example:
        ```env
        LITELLM_MODEL_NAME="mistral/mistral-small-latest"
        # Or for OpenAI: LITELLM_MODEL_NAME="gpt-3.5-turbo"
        ```
    The `intent_analyzer.py` will be refactored to use LiteLLM and will require appropriate API keys and model specification for the chosen backend (e.g., Mistral, OpenAI, etc.). The `main.py` in `orchestrator_agent` will demonstrate loading these.
