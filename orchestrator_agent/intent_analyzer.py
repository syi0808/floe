import os
from typing import Optional, List, Dict, Any # Added Any for the __call__ return type
from agents import Agent, Runner, Tool, Message # Added Message, though not used yet, it's in the plan

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
        # The tool's job is to return the extracted entities.
        # The Runner will capture these and provide them in tool_input.
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
        # The tool's job is to return the extracted entities.
        return {
            "task_description": task_description,
            "due_date": due_date,
            "priority": priority
        }

def extract_intent_and_entities(user_query: str, openai_api_key: str) -> dict:
    try:
        # Ensure OPENAI_API_KEY is set
        os.environ["OPENAI_API_KEY"] = openai_api_key

        # 1. Create instances of the defined tools
        schedule_tool = ExtractScheduleInfoTool()
        task_tool = CreateTaskTool()
        tools = [schedule_tool, task_tool]

        # 2. Create an agents.Agent instance
        agent = Agent(
            tools=tools,
            instructions="Your task is to identify the user's intent and extract relevant entities from their query. Use the available tools to structure this information. If the query is about scheduling, use the 'extract_schedule_info' tool. If it's about creating a task, use the 'create_task' tool. If neither, indicate general conversation."
        )

        # 3. Call agents.Runner.run_sync to get the result
        # According to the documentation, the parameter is user_input, not user_query
        result = Runner.run_sync(agent=agent, user_input=user_query)

        # 4. Inspect result.tool_calls
        if result.tool_calls and len(result.tool_calls) > 0:
            tool_call = result.tool_calls[0]
            # The tool_input is the dictionary returned by the tool's __call__ method
            return {"intent": tool_call.tool_name, "entities": tool_call.tool_input}
        elif result.final_output:
            # 5. Handle general conversation
            return {"intent": "general_conversation", "response_text": result.final_output}
        else:
            # Handle cases where no function call was made and no final_output
            return {"error": "Could not determine intent or provide a response."}

    except Exception as e:
        return {"error": f"Could not determine intent: {str(e)}"}

# Example usage:
# if __name__ == '__main__':
#     # Make sure to set your OPENAI_API_KEY environment variable or pass it directly
#     # For example, replace "YOUR_KEY_HERE" with your actual key
#     api_key = os.environ.get("OPENAI_API_KEY", "YOUR_KEY_HERE")
#
#     intent_data_schedule = extract_intent_and_entities(
#         "Schedule a meeting with Jane for tomorrow at 2 PM about the project budget.",
#         openai_api_key=api_key
#     )
#     if intent_data_schedule and "intent" in intent_data_schedule:
#         print(f"Intent (Schedule): {intent_data_schedule['intent']}")
#         if "entities" in intent_data_schedule:
#             print(f"Entities (Schedule): {intent_data_schedule['entities']}")
#         elif "response_text" in intent_data_schedule:
#             print(f"Response (Schedule): {intent_data_schedule['response_text']}")
#         elif "error" in intent_data_schedule:
#             print(f"Error (Schedule): {intent_data_schedule['error']}")
#     print("-" * 20)
#
#     intent_data_task = extract_intent_and_entities(
#         "Remind me to buy milk tomorrow",
#         openai_api_key=api_key
#     )
#     if intent_data_task and "intent" in intent_data_task:
#         print(f"Intent (Task): {intent_data_task['intent']}")
#         if "entities" in intent_data_task:
#             print(f"Entities (Task): {intent_data_task['entities']}")
#         elif "response_text" in intent_data_task:
#             print(f"Response (Task): {intent_data_task['response_text']}")
#         elif "error" in intent_data_task:
#             print(f"Error (Task): {intent_data_task['error']}")
#     print("-" * 20)
#
#     general_query_data = extract_intent_and_entities(
#         "How are you today?",
#         openai_api_key=api_key
#     )
#     if general_query_data and "intent" in general_query_data:
#         print(f"Intent (General): {general_query_data['intent']}")
#         if "response_text" in general_query_data:
#             print(f"Response (General): {general_query_data['response_text']}")
#         elif "error" in general_query_data:
#             print(f"Error (General): {general_query_data['error']}")
