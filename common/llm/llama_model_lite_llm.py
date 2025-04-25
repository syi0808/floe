from google.adk.models.lite_llm import LiteLlm
import os

os.environ["OPENAI_API_KEY"] = "LOCAL_SERVER_KEY"

LlamaModelLiteLlm = LiteLlm(
    model="openai/gemma3_4b_q4",
    api_base="http://localhost:7100/v1",
    extra_headers={},
)
