from python_a2a import AgentNetwork, AIAgentRouter
from lib.client.llm.LlamaCppA2AClient import LlamaCppA2AClient

llamaCppA2AClient = LlamaCppA2AClient()

# Create an agent network
network = AgentNetwork(name="Travel Assistant Network")

# Add agents to the network
network.add("weather", "http://localhost:7001", headers={"Accept": "application/json"})
network.add(
    "ScheduleAgent", "http://localhost:7002", headers={"Accept": "application/json"}
)

# Create a router to intelligently direct queries to the best agent
router = AIAgentRouter(
    llm_client=llamaCppA2AClient,  # LLM for making routing decisions
    agent_network=network,
)

# Route a query to the appropriate agent
agent_name, confidence = router.route_query("일정 요약해줘.")
print(f"Routing to {agent_name} with {confidence:.2f} confidence")

# Get the selected agent and ask the question
agent = network.get_agent(agent_name)
response = agent.ask("오늘의 일정은 뭐야?")
print(f"Response: {response}")

# List all available agents
print("\nAvailable Agents:")

for agent_info in network.list_agents():
    print(f"- {agent_info['name']}: {agent_info['description']}")
