class Agent:
    def __init__(self, *args, instructions: str | None = None, tools: list | None = None, **kwargs):
        self.instructions = instructions
        self.tools = tools or []

class Runner:
    def __init__(self, agent: Agent | None = None):
        self.agent = agent

    @classmethod
    def run_sync(cls, *args, **kwargs):
        class Result:
            tool_calls = []

        return Result()
