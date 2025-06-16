import os
from dotenv import load_dotenv
from orchestrator_agent.intent_analyzer import extract_intent_and_entities

def main():
    # Load environment variables from .env file
    load_dotenv()

    # Check for LITELLM_MODEL_NAME
    model_name_env = os.getenv("LITELLM_MODEL_NAME")
    if not model_name_env:
        print("Warning: LITELLM_MODEL_NAME environment variable is not set. ")
        print("The intent_analyzer will attempt to use a default model or fail if not configured (it currently requires LITELLM_MODEL_NAME).")
        print("Please set LITELLM_MODEL_NAME (e.g., 'mistral/mistral-small-latest' or 'gpt-3.5-turbo')")
        print("and the corresponding API key (e.g., MISTRAL_API_KEY or OPENAI_API_KEY) in your .env file.")
        # Depending on how strictly intent_analyzer handles a missing model_name,
        # we might want to return here. The current intent_analyzer returns an error if it's not set.
        # For now, we'll let it proceed so the user sees that error from the analyzer.
    else:
        print(f"Using LITELLM_MODEL_NAME: {model_name_env}")

    # Note: API keys like OPENAI_API_KEY, MISTRAL_API_KEY, etc., are loaded by load_dotenv()
    # and LiteLLM will pick them up automatically from the environment.
    # No need to explicitly fetch and pass them to extract_intent_and_entities.

    queries = [
        "Schedule a meeting with John for tomorrow at 2 PM about the project budget.",
        "Remind me to buy milk tomorrow",
        "How are you today?",
        "Book a flight to London" # Example of a potentially unhandled intent by current tools
    ]

    for query in queries:
        print(f"\nProcessing query: '{query}'")
        # Call extract_intent_and_entities without the api_key parameter
        intent_data = extract_intent_and_entities(query)

        if 'error' in intent_data:
            print(f"  Error: {intent_data['error']}")
        elif 'intent' in intent_data:
            print(f"  Intent: {intent_data['intent']}")
            if 'entities' in intent_data:
                print(f"  Entities: {intent_data['entities']}")
            elif 'response_text' in intent_data: # For general_conversation
                print(f"  Response: {intent_data['response_text']}")
        else:
            print(f"  Could not process query effectively. Response: {intent_data}")

    # Basic OrchestratorEngine demonstration (can be expanded later)
    # print("\n--- Orchestrator Engine Demo ---")
    # engine = OrchestrationEngine(memory_manager_agent_client=None) # Modified constructor
    # # Example of registering mock agents if you were to test orchestrator_core here
    # # class MockScheduleAgent:
    # #     def process(self, entities, user_id): return {"mock_data": "scheduled", "entities": entities}
    # # engine.register_agent("schedule_mock", MockScheduleAgent(), ["extract_schedule_info"])
    #
    # schedule_intent_example = {
    #    'intent': 'extract_schedule_info',
    #    'entities': {'title': 'Demo Meeting', 'date': 'Next Monday', 'time': '3 PM'}
    # }
    # if engine: # Ensure engine is initialized if uncommenting
    #    response = engine.route_request(schedule_intent_example, 'user_demo')
    #    print(f"Orchestrator response for schedule: {response}")

if __name__ == "__main__":
    main()
