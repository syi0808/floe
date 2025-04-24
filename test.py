from ollama import Client

print(Client().chat(
  model="llama3", 
  messages=[{"role": "user", "content": "Hello!"}], 
  options={"temperature": 0.5})
)