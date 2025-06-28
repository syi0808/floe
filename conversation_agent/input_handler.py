import datetime
from typing import Optional, Dict, Any

from langdetect import detect, LangDetectException

from .common_types import UserInput

class InputHandler:
    def __init__(self):
        pass

    def process_input(self, text_input: str, metadata: Optional[Dict[str, Any]] = None) -> UserInput:
        """
        Processes raw text input into a UserInput object.
        Basic implementation, can be expanded for more preprocessing.
        """
        # Normalise unicode and collapse whitespace including tabs/newlines
        import unicodedata

        normalised = unicodedata.normalize("NFC", text_input)
        collapsed = " ".join(normalised.replace("\t", " ").split())
        processed_text = collapsed.strip()

        language_code = None
        try:
            if processed_text and len(processed_text) >= 2:
                language_code = detect(processed_text)
        except LangDetectException:
            language_code = None

        if metadata is None:
            metadata = {}
        else:
            metadata = dict(metadata)
        if language_code:
            metadata["language"] = language_code

        return UserInput(
            text=processed_text,
            timestamp=datetime.datetime.utcnow(),
            metadata=metadata
        )

if __name__ == '__main__':
    # Example Usage
    handler = InputHandler()
    user_query = "  Hello, how are you today?   "
    parsed_input = handler.process_input(user_query, metadata={"source": "cli"})
    print(f"Original: '{user_query}'")
    print(f"Processed Text: '{parsed_input.text}'")
    print(f"Timestamp: {parsed_input.timestamp}")
    print(f"Metadata: {parsed_input.metadata}")

    parsed_input_no_meta = handler.process_input("Another query")
    print(f"Processed Text: '{parsed_input_no_meta.text}'")
    print(f"Timestamp: {parsed_input_no_meta.timestamp}")
    print(f"Metadata: {parsed_input_no_meta.metadata}")
