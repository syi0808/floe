import os
from dotenv import load_dotenv
from orchestrator_agent.intent_analyzer import extract_intent_and_entities
from orchestrator_agent.orchestrator_core import OrchestrationEngine, AgentResponse
from orchestrator_agent.base_agent import BaseAgent
from memory_manager_agent.memory_manager import MemoryManagerAgent # Assuming this path is correct

from typing import Dict, Any, List # Ensure these are imported

# --- Mock Agent Implementations ---
# (These will be simple mocks for demonstration in main.py)

class MockMainScheduleAgent(BaseAgent):
    @property
    def name(self) -> str:
        return "mock_main_schedule_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["extract_schedule_info"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        print(f"--- {self.name} processing ---")
        print(f"User ID: {user_id}")
        print(f"Entities: {entities}")
        # Simulate some processing
        confirmation_id = f"sched_{hash(str(entities)) % 10000}"
        return AgentResponse(
            status='success',
            data={'confirmation_id': confirmation_id, 'processed_entities': entities, 'user_id': user_id},
            message=f'Successfully scheduled event: {entities.get("title", "Unknown Event")}',
            source_agent=self.name
        )

class MockMainTaskAgent(BaseAgent):
    @property
    def name(self) -> str:
        return "mock_main_task_agent"

    @property
    def supported_intents(self) -> List[str]:
        return ["create_task"]

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        print(f"--- {self.name} processing ---")
        print(f"User ID: {user_id}")
        print(f"Entities: {entities}")
        # Simulate some processing
        task_id = f"task_{hash(str(entities)) % 10000}"
        return AgentResponse(
            status='success',
            data={'task_id': task_id, 'processed_entities': entities, 'user_id': user_id},
            message=f'Successfully created task: {entities.get("task_description", "Unknown Task")}',
            source_agent=self.name
        )

# --- Main Application Logic ---
def main():
    # Load environment variables from .env file
    load_dotenv()
    print("Floe Orchestrator Agent - Interactive Mode")
    print("Type 'exit' or 'quit' to stop.")
    print("-" * 30)

    # Check for LITELLM_MODEL_NAME (essential for intent_analyzer)
    model_name_env = os.getenv("LITELLM_MODEL_NAME")
    api_key_env_var = None # To store which API key might be relevant
    if model_name_env:
        print(f"Using LITELLM_MODEL_NAME: {model_name_env}")
        # Heuristic to check if a common corresponding API key is also set
        # Map model patterns to their required API key environment variables
        api_key_mapping = {
            "gpt-":    "OPENAI_API_KEY",
            "mistral": "MISTRAL_API_KEY",
            "claude":  "ANTHROPIC_API_KEY",
            "gemini":  "GOOGLE_API_KEY",
            "command": "COHERE_API_KEY",
            "palm":    "GOOGLE_API_KEY",
            # Add more mappings as needed
        }

        for pattern, env_var in api_key_mapping.items():
            if pattern in model_name_env.lower() and not os.getenv(env_var):
                api_key_env_var = env_var
                break

        if api_key_env_var:
            print(f"Warning: LITELLM_MODEL_NAME is set to '{model_name_env}', but the corresponding API key environment variable '{api_key_env_var}' might not be set.")
            print(f"Please ensure '{api_key_env_var}' is set in your .env file.")

    else:
        print("CRITICAL: LITELLM_MODEL_NAME environment variable is not set.")
        print("This is required for intent analysis.")
        print("Please set LITELLM_MODEL_NAME (e.g., 'mistral/mistral-small-latest' or 'gpt-3.5-turbo')")
        print("and the corresponding API key in your .env file.")
        return # Exit if core dependency is missing

    # Initialize MemoryManagerAgent
    memory_manager = MemoryManagerAgent()

    # Initialize OrchestrationEngine with the MemoryManagerAgent
    engine = OrchestrationEngine(memory_manager_client=memory_manager)

    # Instantiate and register mock agents
    schedule_agent = MockMainScheduleAgent()
    task_agent = MockMainTaskAgent()

    engine.register_agent(schedule_agent)
    engine.register_agent(task_agent)

    print("Registered agents:")
    for agent_name, agent_instance in engine.agents_map.items():
        print(f"  - {agent_name} (Handles: {agent_instance.supported_intents})")
    print("-" * 30)

    # Add some initial memory for a test user
    test_user_id = "user_interactive_test"
    memory_manager.add_memory(test_user_id, {"type": "preference", "content": "User prefers brief updates."})
    memory_manager.add_memory(test_user_id, {"type": "past_interaction", "content": "User previously asked about project deadlines."})


    current_user_id = test_user_id # Default user for this interactive session

    while True:
        try:
            user_query = input(f"User ({current_user_id}) > ").strip()
            if user_query.lower() in ['exit', 'quit']:
                print("Exiting Floe Orchestrator Agent.")
                break
            if not user_query:
                continue

            # 1. Extract Intent and Entities
            print("\nAnalyzing intent...")
            intent_data = extract_intent_and_entities(user_query)
            print(f"Intent Analyzer Output: {intent_data}")

            if 'error' in intent_data:
                print(f"  Error during intent analysis: {intent_data['error']}")
                continue

            # 2. Route Request through OrchestrationEngine
            print("\nRouting request...")
            orchestrator_response = engine.route_request(intent_data, current_user_id)
            print(f"Orchestrator Response: {orchestrator_response}")

            # Display the final user-facing message or data from the response
            if orchestrator_response['status'] == 'success':
                if orchestrator_response['source_agent'] == 'OrchestratorAgent' and intent_data['intent'] == 'general_conversation':
                    # General conversation handled by orchestrator directly
                    print(f"\nFloe: {orchestrator_response['data'].get('response', 'Could not generate a response.')}")
                elif 'agent_response' in orchestrator_response['data']:
                    agent_resp_data = orchestrator_response['data']['agent_response']
                    print(f"\nFloe ({agent_resp_data['source_agent']}): {agent_resp_data['message']}")
                    print(f"  Details: {agent_resp_data['data']}")
                else: # Fallback for other orchestrator-handled successful responses
                    print(f"\nFloe: {orchestrator_response['message']}")
                    if orchestrator_response['data']:
                         print(f"  Details: {orchestrator_response['data']}")

            else: # Error from Orchestrator or Agent
                print(f"\nFloe Error: {orchestrator_response['message']}")
                if orchestrator_response['data']:
                     print(f"  Details: {orchestrator_response['data']}")
            print("-" * 30)

        except EOFError: # Handle Ctrl+D
            print("\nExiting Floe Orchestrator Agent (EOF).")
            break
        except KeyboardInterrupt: # Handle Ctrl+C
            print("\nExiting Floe Orchestrator Agent (Interrupt).")
            break
        except Exception as e:
            print(f"\nAn unexpected error occurred: {e}")
            # Optionally, break or continue based on error severity
            # break

if __name__ == "__main__":
    main()
