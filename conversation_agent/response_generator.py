class ResponseGenerator:
    """Generate simple canned responses based on intent."""

    def __init__(self):
        self._responses = {
            "greeting": "Hello! How can I assist you today?",
            "goodbye": "Goodbye! Have a great day.",
            "request_task_creation": "Sure, let's create a task.",
            "ask_question": "I'll look that up for you.",
            "provide_information": "Thank you for letting me know.",
            "unknown": "I'm not sure how to respond to that.",
        }

    def generate_response(self, intent: str) -> str:
        return self._responses.get(intent, self._responses["unknown"])
