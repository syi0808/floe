from llama_cpp.server.app import create_app, ModelSettings, ServerSettings
import uvicorn
import os
import logging
from fastapi import Request, Response
from starlette.background import BackgroundTask

# model_path = os.path.abspath("./models/gemma3_4b_q4.gguf")
# model_alias = "gemma3"
# model_path = os.path.abspath("./models/qwen_7b_q3.gguf")
# model_alias = "qwen"
model_path = os.path.abspath("./models/qwen_2.5_7b_q3_k_m.gguf")
model_alias = "qwen2.5"
# model_path = os.path.abspath("./models/mistral_7b.gguf")
# model_alias = "mistral"
# model_path = os.path.abspath("./models/llama3.2_3b.gguf")
# model_alias = "llama3.2"

host = "0.0.0.0"
port = 7100

app = create_app(
    model_settings=[
        ModelSettings(
            n_ctx=4096,
            model=model_path,
            model_alias=model_alias,
            hf_model_repo_id="Qwen/Qwen2.5-7B",
        )
    ],
    server_settings=ServerSettings(
        host=host,
        port=port,
    ),
)

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("floe_server")


def log_info(req_body, res_body):
    logging.info(req_body)
    logging.info(res_body)


@app.middleware("http")
async def some_middleware(request: Request, call_next):
    req_body = await request.body()
    response = await call_next(request)

    chunks = []
    async for chunk in response.body_iterator:
        chunks.append(chunk)
    res_body = b"".join(chunks)

    task = BackgroundTask(log_info, req_body, res_body)
    return Response(
        content=res_body,
        status_code=response.status_code,
        headers=dict(response.headers),
        media_type=response.media_type,
        background=task,
    )


if __name__ == "__main__":
    uvicorn.run(app, host=host, port=port)
