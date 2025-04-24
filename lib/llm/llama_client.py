import requests


class LlamaClient:
    """HTTP client for the Flask-based Llama server (no init API)."""

    def __init__(self, base_url: str = "http://localhost:7100"):
        self.base_url = base_url.rstrip("/")

    def chat(
        self,
        messages,
        temperature: float = 0.7,
        tools=None,
        functions=None,
        function_call: str = "auto",
    ) -> dict:
        """
        Send chat messages to the Llama server and get completion response.

        Parameters
        ----------
        messages : list of dict
            A list of message dicts in OpenAI format, e.g. {'role': ..., 'content': ...}
        temperature : float, default=0.7
            Sampling temperature
        tools : list, optional
            List of tool definitions for function calling
        functions : list, optional
            OpenAI-style function schema definitions
        function_call : str, default 'auto'
            Mode for function calling
        """
        payload = {
            "messages": messages,
            "temperature": temperature,
            "tools": tools,
            "functions": functions,
            "function_call": function_call,
        }
        response = requests.post(f"{self.base_url}/chat", json=payload)
        response.raise_for_status()
        return response.json()
