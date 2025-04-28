from google.adk.models.lite_llm import LiteLlm
import os
import litellm

litellm._turn_on_debug()

os.environ["OPENAI_API_KEY"] = "LOCAL_SERVER_KEY"

LlamaModelLiteLlm = LiteLlm(
    model="openai/qwen2.5",
    api_base="http://localhost:7100/v1",
    extra_headers={},
)
