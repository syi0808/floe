import os
import json
import litellm
from typing import Optional, List, Dict, Any
# Tool base class is kept for its structure, though not directly used by LiteLLM in the same way
from agents import Tool # Keep Tool for structure if ExtractScheduleInfoTool/CreateTaskTool inherit from it

# The classes ExtractScheduleInfoTool and CreateTaskTool can remain as they are,
# as their .name, .description, and .parameters attributes are used to build the
# JSON for LiteLLM. Their __call__ methods are not used in this LiteLLM-based approach.

class ExtractScheduleInfoTool(Tool):
    def __init__(self):
        self.name = "extract_schedule_info"
        self.description = "Extracts information for scheduling an event."
        self.parameters = {
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
        super().__init__(name=self.name, description=self.description, parameters=self.parameters)

    def __call__(self, title: str, participants: List[str], time: str, date: str, description: Optional[str] = None) -> Dict[str, Any]:
        # This method is not directly called by LiteLLM's tool usage mechanism.
        # LiteLLM expects the LLM to return arguments for the function.
        return {
            "title": title,
            "participants": participants,
            "time": time,
            "date": date,
            "description": description
        }

class CreateTaskTool(Tool):
    def __init__(self):
        self.name = "create_task"
        self.description = "Creates a new task."
        self.parameters = {
            "type": "object",
            "properties": {
                "task_description": {"type": "string", "description": "The description of the task."},
                "due_date": {"type": "string", "description": "Optional due date for the task."},
                "priority": {"type": "string", "description": "Optional priority for the task (e.g., high, medium, low)."}
            },
            "required": ["task_description"]
        }
        super().__init__(name=self.name, description=self.description, parameters=self.parameters)

    def __call__(self, task_description: str, due_date: Optional[str] = None, priority: Optional[str] = None) -> Dict[str, Any]:
        # This method is not directly called by LiteLLM's tool usage mechanism.
        return {
            "task_description": task_description,
            "due_date": due_date,
            "priority": priority
        }

def extract_intent_and_entities(user_query: str) -> Dict[str, Any]:
    # 1. Prepare Tools for LiteLLM
    schedule_tool = ExtractScheduleInfoTool()
    task_tool = CreateTaskTool()

    tools_for_litellm = [
        {
            "type": "function",
            "function": {
                "name": schedule_tool.name,
                "description": schedule_tool.description,
                "parameters": schedule_tool.parameters
            }
        },
        {
            "type": "function",
            "function": {
                "name": task_tool.name,
                "description": task_tool.description,
                "parameters": task_tool.parameters
            }
        }
    ]

    # 2. Determine Model (LiteLLM handles API keys via environment variables)
    # Ensure LITELLM_MODEL_NAME and relevant API key (e.g. MISTRAL_API_KEY, OPENAI_API_KEY) are set in .env
    model_name = os.getenv("LITELLM_MODEL_NAME")
    if not model_name:
        # Fallback or error if no model is specified in the environment
        # For now, let's default to a common one, though ideally this should be configured.
        # print("Warning: LITELLM_MODEL_NAME not set, defaulting to gpt-3.5-turbo. Please set in .env")
        # model_name = "gpt-3.5-turbo"
        # Or, more strictly:
        return {"error": "LITELLM_MODEL_NAME environment variable not set."}


    # 3. Construct Messages Payload
    messages = [
        {
            "role": "system",
            "content": "Your task is to identify the user's intent and extract relevant entities from their query. Use the available tools (functions) to structure this information. If the query is about scheduling, use the 'extract_schedule_info' tool. If it's about creating a task, use the 'create_task' tool. If neither, provide a general response."
        },
        {"role": "user", "content": user_query}
    ]

    # 4. Call litellm.completion
    try:
        # print(f"Attempting LiteLLM completion with model: {model_name}") # Debug
        response = litellm.completion(
            model=model_name,
            messages=messages,
            tools=tools_for_litellm,
            # tool_choice="auto", # Usually default
        )
        # print(f"LiteLLM Raw Response: {response}") # Debug
    except Exception as e:
        # print(f"Error during LiteLLM completion: {e}") # Debug
        return {"error": f"LiteLLM API call failed: {str(e)}"}

    # 5. Process LiteLLM Response
    if response and response.choices and response.choices[0].message:
        message = response.choices[0].message

        if message.tool_calls and len(message.tool_calls) > 0:
            tool_call = message.tool_calls[0] # Assuming one tool call for now
            tool_name = tool_call.function.name
            tool_arguments_str = tool_call.function.arguments

            try:
                parsed_arguments = json.loads(tool_arguments_str)
                return {"intent": tool_name, "entities": parsed_arguments}
            except json.JSONDecodeError as e:
                # print(f"Error decoding tool arguments: {e}") # Debug
                return {"error": f"Failed to parse arguments for tool {tool_name}: {str(e)}"}
        elif message.content: # Check if there's text content
            return {"intent": "general_conversation", "response_text": message.content}
        else:
            # This case might occur if the LLM calls a tool but provides no arguments,
            # or if the response is malformed in an unexpected way.
             return {"error": "LLM responded with a tool call but no valid content or arguments."}


    return {"error": "Could not determine intent or extract useful information from LLM response."}


# Example usage:
# if __name__ == '__main__':
#     from dotenv import load_dotenv # Make sure to import load_dotenv
#     # Ensure your .env file has LITELLM_MODEL_NAME (e.g., "mistral/mistral-small-latest" or "gpt-3.5-turbo")
#     # and the corresponding API key (e.g., MISTRAL_API_KEY or OPENAI_API_KEY)
#     load_dotenv()
#
#     queries = [
#         "Schedule a meeting with Jane for tomorrow at 2 PM about the project budget.",
#         "Remind me to buy milk tomorrow",
#         "How are you today?"
#     ]
#     for query in queries:
#         print(f"Testing query: {query}")
#         result = extract_intent_and_entities(query)
#         print(f"Result: {result}\n")
