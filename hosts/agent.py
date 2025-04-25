from .host_agent import HostAgent

root_agent = HostAgent(["http://localhost:10001"]).create_agent()
