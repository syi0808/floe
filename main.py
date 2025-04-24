import os
from python_a2a import AgentNetwork, AIAgentRouter
from lib.client.llm.LlamaCppA2AClient import LlamaCppA2AClient

llamaCppA2AClient = LlamaCppA2AClient(
    model_path=os.path.abspath("./models/gemma3_4b_q4.gguf")
)

# Create an agent network
network = AgentNetwork(name="Travel Assistant Network")

# Add agents to the network
network.add("weather", "http://localhost:7001", headers={"Accept": "application/json"})

# Create a router to intelligently direct queries to the best agent
router = AIAgentRouter(
    llm_client=llamaCppA2AClient,  # LLM for making routing decisions
    agent_network=network,
)

# Route a query to the appropriate agent
agent_name, confidence = router.route_query("What's the weadfvndkjfnvtherdfvdfv?")
print(f"Routing to {agent_name} with {confidence:.2f} confidence")

# Get the selected agent and ask the question
agent = network.get_agent(agent_name)
response = agent.ask("What's the weather like in Paris?")
print(f"Response: {response}")

# List all available agents
print("\nAvailable Agents:")

for agent_info in network.list_agents():
    print(f"- {agent_info['name']}: {agent_info['description']}")
