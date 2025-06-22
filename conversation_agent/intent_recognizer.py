import re
from typing import List

class IntentRecognizer:
    """Very simple rule-based intent recognizer."""

    def __init__(self):
        # Mapping of intent names to keyword lists
        self._keyword_map = {
            "greeting": ["hello", "hi", "hey"],
            "goodbye": ["bye", "goodbye", "see you"],
            "request_task_creation": ["create task", "add task", "new task"],
        }

    def recognize_intent(self, text: str) -> str:
        text_l = text.lower()
        for intent, keywords in self._keyword_map.items():
            for kw in keywords:
                if kw in text_l:
                    return intent
        # Basic question detection
        if text_l.strip().endswith("?") or re.match(r"^(what|how|why|when)\b", text_l):
            return "ask_question"
        return "unknown"
