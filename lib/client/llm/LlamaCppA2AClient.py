from typing import Optional, List, Dict, Any
import json

from python_a2a import (
    Message,
    MessageRole,
    TextContent,
    FunctionCallContent,
    FunctionParameter,
    Conversation,
    BaseA2AClient,
    A2AImportError,
)

# Import the HTTP-based Llama client
from lib.llm.llama_client import LlamaClient

__all__ = ["LlamaCppA2AClient"]


class LlamaCppA2AClient(BaseA2AClient):
    """A2A client that delegates to a remote Llama server via HTTP.

    Uses the Flask-based `LlamaClient` API under the hood.

    Parameters
    ----------
    base_url : str
        URL of the running Llama server (e.g. 'http://localhost:7100').
    temperature : float, default=0.7
        Sampling temperature for generation.
    system_prompt : Optional[str]
        Optional system message prepended to every chat.
    functions : Optional[List[Dict[str, Any]]]
        Optional list of JSON-schema function definitions for
        OpenAI-style function calling.
    """

    def __init__(
        self,
        base_url: str = "http://localhost:7100",
        temperature: float = 0.7,
        *,
        system_prompt: Optional[str] = None,
        functions: Optional[List[Dict[str, Any]]] = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.temperature = temperature
        self.system_prompt = system_prompt or "You are a helpful assistant."
        self.functions = functions
        self.tools = self._convert_functions_to_tools() if functions else None

        # Initialize HTTP client
        try:
            self.llama = LlamaClient(base_url=self.base_url)
        except Exception as exc:
            raise A2AImportError(f"Could not initialize HTTP Llama client: {exc}")

        # conversation-id → list of message dicts
        self._conversation_histories: Dict[str, List[Dict[str, Any]]] = {}

    # ---------------------------------------------------------------------
    # helpers
    # ---------------------------------------------------------------------
    def _convert_functions_to_tools(self) -> List[Dict[str, Any]]:
        return [{"type": "function", "function": f} for f in (self.functions or [])]

    def _append_message_to_history(
        self, conversation_id: Optional[str], message: Dict[str, Any]
    ) -> None:
        if conversation_id is None:
            return
        history = self._conversation_histories.setdefault(
            conversation_id, [{"role": "system", "content": self.system_prompt}]
        )
        history.append(message)

    def _prepare_messages(
        self, base: List[Dict[str, Any]], message: Message
    ) -> List[Dict[str, Any]]:
        if message.content.type == "text":
            base.append(
                {
                    "role": "user" if message.role == MessageRole.USER else "assistant",
                    "content": message.content.text,
                }
            )
        elif message.content.type == "function_call":
            params_str = ", ".join(
                f"{p.name}={p.value}" for p in message.content.parameters
            )
            base.append(
                {
                    "role": "user",
                    "content": f"Call function {message.content.name}({params_str})",
                }
            )
        elif message.content.type == "function_response":
            base.append(
                {
                    "role": "function",
                    "name": message.content.name,
                    "content": json.dumps(message.content.response),
                }
            )
        else:
            base.append({"role": "user", "content": str(message.content)})
        return base

    def _chat(
        self,
        messages: List[Dict[str, Any]],
        temperature: float,
    ) -> Dict[str, Any]:
        # Delegate to HTTP client
        return self.llama.chat(
            messages=messages,
            temperature=temperature,
            tools=self.tools,
            functions=self.functions,
            function_call="auto",
        )

    def _parse_response_message(
        self,
        resp_msg: Dict[str, Any],
        *,
        parent_id: Optional[str],
        conv_id: Optional[str],
    ) -> Message:
        if "function_call" in resp_msg and resp_msg.get("function_call"):
            fc = resp_msg["function_call"]
            try:
                args = json.loads(fc.get("arguments", "{}"))
                parameters = [
                    FunctionParameter(name=k, value=v) for k, v in args.items()
                ]
            except Exception:
                parameters = [
                    FunctionParameter(name="arguments", value=fc.get("arguments", ""))
                ]
            return Message(
                content=FunctionCallContent(name=fc.get("name"), parameters=parameters),
                role=MessageRole.AGENT,
                parent_message_id=parent_id,
                conversation_id=conv_id,
            )
        return Message(
            content=TextContent(text=resp_msg.get("content", "")),
            role=MessageRole.AGENT,
            parent_message_id=parent_id,
            conversation_id=conv_id,
        )

    # ------------------------------------------------------------------
    # public API – BaseA2AClient overrides
    # ------------------------------------------------------------------
    def send_message(self, message: Message) -> Message:
        try:
            history = self._conversation_histories.get(
                message.conversation_id,
                [{"role": "system", "content": self.system_prompt}],
            )
            prompt_msgs = self._prepare_messages(history.copy(), message)

            response = self._chat(
                messages=prompt_msgs,
                temperature=self.temperature,
            )
            resp_msg = response["choices"][0]["message"]

            self._append_message_to_history(message.conversation_id, prompt_msgs[-1])
            a2a_response = self._parse_response_message(
                resp_msg,
                parent_id=message.message_id,
                conv_id=message.conversation_id,
            )
            assistant_dict = {
                k: v
                for k, v in resp_msg.items()
                if k in {"role", "content", "function_call"}
            }
            self._append_message_to_history(message.conversation_id, assistant_dict)
            return a2a_response
        except Exception as exc:
            return Message(
                content=TextContent(text=f"Error from HTTP Llama client: {exc}"),
                role=MessageRole.AGENT,
                parent_message_id=message.message_id,
                conversation_id=message.conversation_id,
            )

    def send_conversation(self, conversation: Conversation) -> Conversation:
        if not conversation.messages:
            return conversation
        try:
            prompt_msgs: List[Dict[str, Any]] = [
                {"role": "system", "content": self.system_prompt}
            ]
            for m in conversation.messages:
                prompt_msgs = self._prepare_messages(prompt_msgs, m)

            response = self._chat(
                messages=prompt_msgs,
                temperature=self.temperature,
            )
            resp_msg = response["choices"][0]["message"]

            last_msg = conversation.messages[-1]
            a2a_msg = self._parse_response_message(
                resp_msg,
                parent_id=last_msg.message_id,
                conv_id=conversation.conversation_id,
            )
            conversation.add_message(a2a_msg)
            self._conversation_histories[conversation.conversation_id] = prompt_msgs + [
                resp_msg
            ]
            return conversation
        except Exception as exc:
            conversation.create_error_message(
                f"Error from HTTP Llama client: {exc}",
                parent_message_id=(
                    conversation.messages[-1].message_id
                    if conversation.messages
                    else None
                ),
            )
            return conversation

    def ask(self, query: str) -> str:
        message = Message(content=TextContent(text=query), role=MessageRole.USER)
        response = self.send_message(message)
        if response.content.type == "text":
            return response.content.text
        if response.content.type == "function_call":
            params = ", ".join(
                f"{p.name}={p.value}" for p in response.content.parameters
            )
            return f"Function call: {response.content.name}({params})"
        return str(response.content)

    def clear_conversation_history(self, conversation_id: Optional[str] = None) -> None:
        if conversation_id is None:
            self._conversation_histories.clear()
        else:
            self._conversation_histories.pop(conversation_id, None)
