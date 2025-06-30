import datetime
from typing import Optional, Dict, Any, Union

from langdetect import detect, LangDetectException

from .common_types import UserInput

class InputHandler:
    def __init__(self):
        pass

    def process_input(
        self,
        text_input: Union[str, Dict[str, Any]],
        metadata: Optional[Dict[str, Any]] = None,
    ) -> UserInput:
        """Parse raw text or message payload into a ``UserInput`` object."""

        input_metadata: Dict[str, Any] = {}
        timestamp = datetime.datetime.utcnow()

        if isinstance(text_input, dict):
            raw_text = text_input.get("text") or text_input.get("message") or ""
            input_metadata.update(text_input.get("metadata", {}))
            ts_val = text_input.get("timestamp")
            if ts_val:
                try:
                    timestamp = datetime.datetime.fromisoformat(ts_val)
                except Exception:
                    pass
            if "language" in text_input:
                input_metadata["language"] = text_input["language"]
        else:
            raw_text = text_input

        if metadata:
            input_metadata.update(metadata)

        # Normalise unicode and collapse whitespace including tabs/newlines
        import unicodedata

        normalised = unicodedata.normalize("NFC", str(raw_text))
        collapsed = " ".join(normalised.replace("\t", " ").split())
        processed_text = collapsed.strip()

        if "language" not in input_metadata:
            try:
                if processed_text and len(processed_text) >= 2:
                    lang = detect(processed_text)
                    input_metadata["language"] = lang
            except LangDetectException:
                pass

        return UserInput(text=processed_text, timestamp=timestamp, metadata=input_metadata)

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
