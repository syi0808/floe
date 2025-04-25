from llama_cpp.server.app import create_app, ModelSettings, ServerSettings
import uvicorn
import os

model_path = os.path.abspath("./models/gemma3_4b_q4.gguf")
host = "0.0.0.0"
port = 7100

print(model_path)

app = create_app(
    model_settings=[
        ModelSettings(
            n_ctx=4096,
            model=model_path,
            model_alias="gemma3_4b_q4",
        )
    ],
    server_settings=ServerSettings(
        host=host,
        port=port,
    ),
)

if __name__ == "__main__":
    uvicorn.run(app, host=host, port=port)
