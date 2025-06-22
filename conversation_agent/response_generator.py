class ResponseGenerator:
    """Generate simple canned responses based on intent and language."""

    def __init__(self):
        self._responses = {
            "greeting": {
                "en": "Hello! How can I assist you today?",
                "ko": "안녕하세요! 무엇을 도와드릴까요?",
            },
            "goodbye": {
                "en": "Goodbye! Have a great day.",
                "ko": "안녕히 가세요! 좋은 하루 되세요.",
            },
            "request_task_creation": {
                "en": "Sure, let's create a task.",
                "ko": "알겠습니다. 할 일을 만들어 보겠습니다.",
            },
            "ask_question": {
                "en": "I'll look that up for you.",
                "ko": "찾아봐 드리겠습니다.",
            },
            "provide_information": {
                "en": "Thank you for letting me know.",
                "ko": "알려주셔서 감사합니다.",
            },
            "unknown": {
                "en": "I'm not sure how to respond to that.",
                "ko": "어떻게 답해야 할지 모르겠어요.",
            },
        }

    def generate_response(self, intent: str, language: str = "en") -> str:
        intent_map = self._responses.get(intent, self._responses["unknown"])
        return intent_map.get(language, intent_map["en"])
