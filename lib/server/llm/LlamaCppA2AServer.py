"""
Llama-based server implementation for the A2A protocol.
"""

import uuid
import json
from typing import Optional, Dict, Any, List

try:
    from lib.llm.llama_client import LlamaClient
except ImportError:
    LlamaClient = None

from python_a2a import (
    Message,
    MessageRole,
    TextContent,
    FunctionCallContent,
    FunctionResponseContent,
    Task,
    TaskStatus,
    TaskState,
    Conversation,
    A2AConnectionError,
    A2AImportError,
    BaseA2AServer,
)


class LlamaCppA2AServer(BaseA2AServer):
    """
    An A2A server that uses a remote Llama HTTP server to process messages.
    """

    def __init__(
        self,
        base_url: str = "http://localhost:7100",
        temperature: float = 0.7,
        system_prompt: Optional[str] = None,
        functions: Optional[List[Dict[str, Any]]] = None,
    ):
        if LlamaClient is None:
            raise A2AImportError(
                "Llama HTTP client is not available. "
                "Ensure llama_cpp_http_client is installed and importable."
            )
        self.client = LlamaClient(base_url=base_url)
        self.temperature = temperature
        self.system_prompt = system_prompt or "You are a helpful AI assistant."
        self.functions = functions
        self.tools = self._convert_functions_to_tools() if functions else None

        # Track conversation state: id -> list of OpenAI-format messages
        self._conversation_state: Dict[str, List[Dict[str, Any]]] = {}

    def _convert_functions_to_tools(self) -> List[Dict[str, Any]]:
        return [{"type": "function", "function": f} for f in (self.functions or [])]

    def handle_message(self, message: Message) -> Message:
        """
        Process a single message via Llama HTTP client.
        """
        try:
            convo_id = message.conversation_id or str(uuid.uuid4())
            # initialize history if new
            history = self._conversation_state.setdefault(
                convo_id, [{"role": "system", "content": self.system_prompt}]
            )
            # append user or assistant
            if message.content.type == "text":
                history.append(
                    {
                        "role": (
                            "user" if message.role == MessageRole.USER else "assistant"
                        ),
                        "content": message.content.text,
                    }
                )
            elif message.content.type == "function_call":
                args_str = ", ".join(
                    f"{p.name}={p.value}" for p in message.content.parameters
                )
                history.append(
                    {
                        "role": "user",
                        "content": f"Call function {message.content.name}({args_str})",
                    }
                )
            elif message.content.type == "function_response":
                history.append(
                    {
                        "role": "function",
                        "name": message.content.name,
                        "content": json.dumps(message.content.response),
                    }
                )
            # send to HTTP Llama
            resp = self.client.chat(
                messages=history,
                temperature=self.temperature,
                tools=self.tools,
                functions=self.functions,
                function_call="auto",
            )
            choice = resp["choices"][0]["message"]
            # record assistant response
            if "content" in choice:
                history.append(
                    {
                        "role": choice.get("role", "assistant"),
                        "content": choice.get("content", ""),
                    }
                )
            if "function_call" in choice and choice["function_call"]:
                # also append function_call role if exists
                history.append(
                    {"role": "assistant", "function_call": choice["function_call"]}
                )
            # parse back to A2A Message
            if choice.get("function_call"):
                fc = choice["function_call"]
                try:
                    args = json.loads(fc.get("arguments", "{}"))
                    params = [
                        FunctionResponseContent(name=k, response=v)
                        for k, v in args.items()
                    ]
                except Exception:
                    params = []
                return Message(
                    content=FunctionCallContent(name=fc.get("name"), parameters=params),
                    role=MessageRole.AGENT,
                    parent_message_id=message.message_id,
                    conversation_id=convo_id,
                )
            return Message(
                content=TextContent(text=choice.get("content", "")),
                role=MessageRole.AGENT,
                parent_message_id=message.message_id,
                conversation_id=convo_id,
            )
        except Exception as e:
            raise A2AConnectionError(f"Llama HTTP server error: {e}")

    def handle_task(self, task: Task) -> Task:
        try:
            msg = (
                task.message
                if isinstance(task.message, Message)
                else Message.from_dict(task.message)
            )
            response = self.handle_message(msg)
            # set artifacts based on response
            content = response.content
            if isinstance(content, TextContent):
                task.artifacts = [{"parts": [{"type": "text", "text": content.text}]}]
            elif isinstance(content, FunctionResponseContent):
                task.artifacts = [
                    {
                        "parts": [
                            {
                                "type": "function_response",
                                "name": content.name,
                                "response": content.response,
                            }
                        ]
                    }
                ]
            elif isinstance(content, FunctionCallContent):
                task.artifacts = [
                    {
                        "parts": [
                            {
                                "type": "function_call",
                                "name": content.name,
                                "parameters": [p.__dict__ for p in content.parameters],
                            }
                        ]
                    }
                ]
            task.status = TaskStatus(state=TaskState.COMPLETED)
            return task
        except Exception as e:
            task.artifacts = [{"parts": [{"type": "error", "message": str(e)}]}]
            task.status = TaskStatus(state=TaskState.FAILED)
            return task

    def handle_conversation(self, conversation: Conversation) -> Conversation:
        try:
            for msg in conversation.messages:
                # seed history
                _ = self.handle_message(msg)
            return conversation
        except Exception as e:
            conversation.create_error_message(
                str(e), parent_message_id=conversation.messages[-1].message_id
            )
            return conversation

    def get_metadata(self) -> Dict[str, Any]:
        md = super().get_metadata()
        md.update({"agent_type": "LlamaCppA2AServer", "capabilities": ["text"]})
        if self.functions:
            md["capabilities"].append("function_calling")
            md["functions"] = [f["name"] for f in self.functions]
        return md
