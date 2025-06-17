import os
from typing import Optional, Dict, Any, List
# Check if this import is correct based on the actual 'agents' SDK structure
# It might be 'from agents_sdk import Agent, Runner, Tool' or similar
# For now, let's assume 'from agents import Agent, Runner, Tool' is correct as per docs
from agents import Agent, Runner, Tool

class CreateTaskFromDetailsTool(Tool):
    def __init__(self):
        self.name = "create_task_from_details"
        self.description = "Extracts task details like description, due date, and priority from natural language."
        self.parameters = {
            "type": "object",
            "properties": {
                "description": {"type": "string", "description": "The full description of the task."},
                "due_date": {"type": "string", "description": "Optional due date (e.g., 'tomorrow', 'end of week', 'July 20th')."},
                "priority": {"type": "string", "enum": ["high", "medium", "low"], "description": "Optional task priority."},
                "project": {"type": "string", "description": "Optional project or category for the task."}
            },
            "required": ["description"]
        }
        super().__init__(name=self.name, description=self.description, parameters=self.parameters)

    def __call__(self, description: str, due_date: Optional[str] = None,
                 priority: Optional[str] = None, project: Optional[str] = None) -> Dict[str, Any]:
        # This method is called by the agent runner with extracted arguments.
        # It should simply return these arguments as a dictionary.
        return {
            "description": description,
            "due_date": due_date,
            "priority": priority,
            "project": project
        }

def parse_task_request(natural_language_query: str, context_document: Optional[str] = None) -> Optional[Dict[str, Any]]:
    # Note: context_document is not used in this specific tool example from the plan,
    # but the signature includes it for future extensibility (e.g., extracting from larger texts).
    try:
        task_tool = CreateTaskFromDetailsTool()

        # Instructions for the agent
        # These instructions guide the LLM on how to behave and when to use the tool.
        agent_instructions = (
            "You are an assistant specializing in task management. "
            "Your primary function is to extract structured task information from user queries. "
            "Use the 'create_task_from_details' tool to parse the user's input into distinct fields: "
            "description, due_date, priority, and project. "
            "Focus solely on identifying these details. Do not perform any other actions or generate conversational replies."
        )

        agent = Agent(
            tools=[task_tool],
            instructions=agent_instructions
            # Forcing tool usage can also be done by setting tool_choice on the model_settings
            # or by very specific instructions if the SDK supports it directly.
            # The current approach relies on strong instructions and a single tool.
        )

        # The user_input for the Runner should be the natural language query.
        result = Runner.run_sync(agent=agent, user_input=natural_language_query)

        if result.tool_calls and len(result.tool_calls) > 0:
            # Assuming the first tool call is the one we're interested in,
            # as we only provided one tool.
            tool_call = result.tool_calls[0]
            if tool_call.tool_name == "create_task_from_details":
                # tool_input is the dictionary returned by CreateTaskFromDetailsTool.__call__
                # It contains the arguments extracted by the LLM and passed to the tool.
                return tool_call.tool_input

        # Fallback if no tool call was made or not the expected one.
        # This might indicate the LLM couldn't map the query to the tool,
        # or the query was too vague.
        # print(f"Debug: No tool call made or unexpected tool. Final output: {result.final_output}") # Optional: for debugging
        return None
    except Exception as e:
        # print(f"Error in parsing task details: {e}") # Optional: for debugging
        return None

# Example usage (for testing purposes, can be run if this file is executed directly):
# if __name__ == '__main__':
#     # Ensure OPENAI_API_KEY is set in your environment for this example to run
#     # os.environ["OPENAI_API_KEY"] = "your_openai_api_key_here"
#     # os.environ["LITELLM_MODEL_NAME"] = "gpt-3.5-turbo" # Or any other supported model

#     print("Attempting to parse task queries. Ensure your LLM environment variables are set.")

#     queries = [
#         "Add 'Finish the report for Project Alpha' to my tasks, it's due next Friday and is high priority.",
#         "Remind me to buy milk tomorrow.",
#         "Need to schedule a dentist appointment for next week, medium priority, under personal project.",
#         "This is just a random sentence not related to tasks."
#     ]

#     for query in queries:
#         print(f"\nProcessing query: '{query}'")
#         task_details = parse_task_request(query)
#         if task_details:
#             print(f"  Parsed task details: {task_details}")
#         else:
#             print("  Could not parse task details from this query.")
