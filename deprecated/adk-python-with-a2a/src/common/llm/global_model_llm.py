# from google.adk.models.lite_llm import LiteLlm
# import os

# api_key = "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"

# os.environ["OPENROUTER_API_KEY"] = api_key

# GlobalModelLlm = LiteLlm(
#     model="openrouter/google/gemini-2.0-flash-exp:free",
#     base_url="https://openrouter.ai/api/v1",
# )

from google.adk.models.lite_llm import LiteLlm
import os

api_key = "cGifFm5pImXlgsdSnor4RuGaRRmp9tAj"

os.environ["MISTRAL_API_KEY"] = api_key

GlobalModelLlm = LiteLlm(
    model="mistral/mistral-small-latest",
    base_url="https://api.mistral.ai/v1",
)
