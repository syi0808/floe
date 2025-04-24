from __future__ import annotations

"""Llama‑cpp‑python based client implementation for the A2A protocol.

This mirrors the behaviour of ``OpenAIA2AClient`` but executes the model
locally via ``llama‑cpp‑python`` so you can swap the backend without
changing the rest of your application code.
"""

from typing import Optional, List, Dict, Any
import json

try:
    from llama_cpp import Llama  # type: ignore
except ImportError:  # pragma: no cover – informative error only at runtime
    Llama = None  # pyright: ignore[reportGeneralTypeIssues]

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

__all__ = ["LlamaCppA2AClient"]


class LlamaCppA2AClient(BaseA2AClient):
    """A2A client that uses a *local* `llama‑cpp‑python` model.

    Parameters
    ----------
    model_path:
        Path to a GGUF/GGML model file compatible with ``llama.cpp``.
    temperature:
        Sampling temperature used for generation.  *(default: 0.7)*
    n_ctx:
        Context length for the model.  *(default: 4096)*
    n_threads:
        Threads to use; ``0`` = #physical cores. *(default: auto)*
    system_prompt:
        Optional system message prepended to every chat.
    functions:
        Optional list of JSON‑schema function definitions for
        OpenAI‑style function calling.
    **llama_kwargs:
        Any additional keyword arguments accepted by
        :class:`llama_cpp.Llama`.
    """

    def __init__(
        self,
        model_path: str,
        *,
        temperature: float = 0.7,
        n_ctx: int = 4096,
        n_threads: int | None = None,
        system_prompt: Optional[str] = None,
        functions: Optional[List[Dict[str, Any]]] = None,
        **llama_kwargs: Any,
    ) -> None:
        if Llama is None:
            raise A2AImportError(
                "llama‑cpp‑python is not installed. Install it with 'pip install llama-cpp-python'"
            )

        self.temperature = temperature
        self.system_prompt = system_prompt or "You are a helpful assistant."
        self.functions = functions
        self.tools = self._convert_functions_to_tools() if functions else None

        # lazy‑load model
        self.llama = Llama(
            model_path=model_path,
            n_ctx=n_ctx,
            n_threads=n_threads or 0,
            **llama_kwargs,
        )

        # conversation‑id → list[openai‑style message dict]
        self._conversation_histories: Dict[str, List[Dict[str, Any]]] = {}

    # ---------------------------------------------------------------------
    # helpers
    # ---------------------------------------------------------------------
    def _convert_functions_to_tools(self) -> List[Dict[str, Any]]:
        """Convert OpenAI "functions" spec to the newer "tools" format."""
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
        """Translate an :class:`~python_a2a.models.message.Message` to OpenAI format."""
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

    def _chat(self, messages: List[Dict[str, Any]]) -> Dict[str, Any]:
        kwargs: Dict[str, Any] = {
            "messages": messages,
            "temperature": self.temperature,
        }
        if self.tools:
            kwargs.update({"tools": self.tools, "tool_choice": "auto"})
        elif self.functions:
            kwargs.update({"functions": self.functions, "function_call": "auto"})
        return self.llama.create_chat_completion(**kwargs)  # type: ignore[arg-type]

    def _parse_response_message(
        self,
        resp_msg: Dict[str, Any],
        *,
        parent_id: Optional[str],
        conv_id: Optional[str],
    ) -> Message:
        """Convert response dict into an A2A :class:`~Message`."""
        if "function_call" in resp_msg and resp_msg["function_call"]:
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
                content=FunctionCallContent(name=fc["name"], parameters=parameters),
                role=MessageRole.AGENT,
                parent_message_id=parent_id,
                conversation_id=conv_id,
            )
        # plain text
        return Message(
            content=TextContent(text=resp_msg.get("content", "")),
            role=MessageRole.AGENT,
            parent_message_id=parent_id,
            conversation_id=conv_id,
        )

    # ------------------------------------------------------------------
    # public API – BaseA2AClient overrides
    # ------------------------------------------------------------------
    def send_message(
        self, message: Message
    ) -> Message:  # noqa: D401 – following base class
        """Send a single message and receive the model's reply."""
        try:
            base_msgs = self._conversation_histories.get(
                message.conversation_id,
                [{"role": "system", "content": self.system_prompt}],
            ).copy()
            prompt_msgs = self._prepare_messages(base_msgs, message)
            response = self._chat(prompt_msgs)
            resp_msg: Dict[str, Any] = response["choices"][0]["message"]  # type: ignore[index]

            # track history
            self._append_message_to_history(message.conversation_id, prompt_msgs[-1])

            a2a_response = self._parse_response_message(
                resp_msg, parent_id=message.message_id, conv_id=message.conversation_id
            )
            # add assistant's reply to history as well
            assistant_dict = {
                k: v
                for k, v in resp_msg.items()
                if k in {"role", "content", "function_call"}
            }
            self._append_message_to_history(message.conversation_id, assistant_dict)
            return a2a_response
        except Exception as exc:  # broad – we want to capture *all* failures
            return Message(
                content=TextContent(text=f"Error from llama‑cpp: {exc}"),
                role=MessageRole.AGENT,
                parent_message_id=message.message_id,
                conversation_id=message.conversation_id,
            )

    def send_conversation(
        self, conversation: Conversation
    ) -> Conversation:  # noqa: D401
        if not conversation.messages:
            return conversation
        try:
            prompt_msgs: List[Dict[str, Any]] = [
                {"role": "system", "content": self.system_prompt}
            ]
            for m in conversation.messages:
                prompt_msgs = self._prepare_messages(prompt_msgs, m)
            response = self._chat(prompt_msgs)
            resp_msg: Dict[str, Any] = response["choices"][0]["message"]  # type: ignore[index]

            last_msg = conversation.messages[-1]
            a2a_msg = self._parse_response_message(
                resp_msg,
                parent_id=last_msg.message_id,
                conv_id=conversation.conversation_id,
            )
            conversation.add_message(a2a_msg)
            self._conversation_histories[conversation.conversation_id] = prompt_msgs + [resp_msg]  # type: ignore[index]
            return conversation
        except Exception as exc:
            conversation.create_error_message(
                f"Error from llama‑cpp: {exc}",
                parent_message_id=(
                    conversation.messages[-1].message_id
                    if conversation.messages
                    else None
                ),
            )
            return conversation

    def ask(self, query: str) -> str:  # noqa: D401
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

    # ------------------------------------------------------------------
    # utilities
    # ------------------------------------------------------------------
    def clear_conversation_history(
        self, conversation_id: str | None = None
    ) -> None:  # noqa: D401
        if conversation_id is None:
            self._conversation_histories.clear()
        else:
            self._conversation_histories.pop(conversation_id, None)
