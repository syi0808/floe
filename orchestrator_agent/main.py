import os
from dotenv import load_dotenv
from orchestrator_agent.intent_analyzer import extract_intent_and_entities
from orchestrator_agent.orchestrator_core import OrchestrationEngine # Added for potential future use

def main():
    # Load environment variables from .env file
    load_dotenv()
    api_key = os.getenv("OPENAI_API_KEY")

    if not api_key:
        print("Error: OPENAI_API_KEY not found. Please set it in your .env file or environment.")
        return

    print("OpenAI API Key loaded.")

    queries = [
        "Schedule a meeting with John for tomorrow at 2 PM about the project budget.",
        "Remind me to buy milk tomorrow",
        "How are you today?",
        "Book a flight to London" # Example of a potentially unhandled intent by current tools
    ]

    for query in queries:
        print(f"\nProcessing query: '{query}'")
        intent_data = extract_intent_and_entities(query, openai_api_key=api_key)

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
    # engine = OrchestrationEngine(memory_manager_agent_client=None, available_agents_map=None)
    # schedule_intent_example = {
    #    'intent': 'extract_schedule_info',
    #    'entities': {'title': 'Demo Meeting', 'date': 'Next Monday', 'time': '3 PM'}
    # }
    # response = engine.route_request(schedule_intent_example, 'user_demo')
    # print(f"Orchestrator response for schedule: {response}")

if __name__ == "__main__":
    main()
