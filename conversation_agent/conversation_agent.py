from conversation_agent.input_handler import InputHandler
from conversation_agent.dialogue_manager import DialogueManager
from conversation_agent.intent_recognizer import IntentRecognizer
from conversation_agent.response_generator import ResponseGenerator
from conversation_agent.common_types import AgentResponse

class ConversationAgent:
    """Simple conversational agent orchestrating input, intent and responses."""

    def __init__(self):
        self.input_handler = InputHandler()
        self.dialogue_manager = DialogueManager()
        self.intent_recognizer = IntentRecognizer()
        self.response_generator = ResponseGenerator()

    def handle_message(self, text: str) -> AgentResponse:
        user_input = self.input_handler.process_input(text)
        self.dialogue_manager.add_user_input(user_input)

        intent = self.intent_recognizer.recognize_intent(user_input.text)
        language = None
        if user_input.metadata:
            language = user_input.metadata.get("language")

        response_text = self.response_generator.generate_response(
            intent, language=language or "en"
        )

        agent_response = AgentResponse(text=response_text)
        self.dialogue_manager.add_agent_response(agent_response)
        return agent_response
