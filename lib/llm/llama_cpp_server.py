from flask import Flask, request, jsonify
from llama_cpp import Llama
import threading
import argparse
import json
import os

app = Flask(__name__)

# Global Llama instance (initialized once at startup)
llama_model = None
model_lock = threading.Lock()

# Initialize model on server start using CLI args or environment variables


def create_model(model_path: str, n_ctx: int, n_threads: int, **llama_kwargs) -> Llama:
    """Instantiate the Llama model with given parameters."""
    return Llama(
        model_path=model_path, n_ctx=n_ctx, n_threads=n_threads, **llama_kwargs
    )


@app.route("/chat", methods=["POST"])
def chat():
    global llama_model
    payload = request.json or {}
    messages = payload.get("messages")
    temperature = payload.get("temperature", 0.7)
    tools = payload.get("tools")
    functions = payload.get("functions")
    function_call = payload.get("function_call", "auto")

    with model_lock:
        if llama_model is None:
            return jsonify({"error": "Model not initialized on server"}), 500
        # Forward payload to llama
        response = llama_model.create_chat_completion(
            messages=messages,
            temperature=temperature,
            tools=tools,
            functions=functions,
            function_call=function_call,
        )
        print(payload)
        print(response)
    return jsonify(response)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Start Llama Flask server")
    parser.add_argument(
        "--model-path",
        required=False,
        default=os.path.abspath("./models/gemma3_4b_q4.gguf"),
        help="Path to GGUF/GGML model file",
    )
    parser.add_argument("--n-ctx", type=int, default=4096, help="Context window size")
    parser.add_argument(
        "--n-threads", type=int, default=0, help="Number of threads (0=auto)"
    )
    parser.add_argument(
        "--llama-kwargs",
        type=json.loads,
        default="{}",
        help="Additional llama-cpp-python kwargs as JSON string",
    )
    args = parser.parse_args()

    # Initialize global model
    llama_model = create_model(
        model_path=args.model_path,
        n_ctx=args.n_ctx,
        n_threads=args.n_threads,
        **args.llama_kwargs,
    )

    app.run(host="0.0.0.0", port=7100)
