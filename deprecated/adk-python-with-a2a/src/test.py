from hosts.host_agent import HostAgent
from google.adk.runners import Runner
from google.adk.sessions import InMemorySessionService
from google.genai import types

root_agent = HostAgent(["http://localhost:10001"]).create_agent()

runner = Runner(
    app_name=root_agent.name,
    agent=root_agent,
    session_service=InMemorySessionService(),
)


if __name__ == "__main__":
    session = runner.session_service.create_session(
        app_name=root_agent.name,
        user_id="test_user",
        state={},
        session_id="temp",
    )

    events = list(
        runner.run(
            user_id="test_user",
            session_id="temp",
            new_message=types.Content(
                role="user", parts=[types.Part.from_text(text="오늘 일정을 요약해줘.")]
            ),
        )
    )

    print(events)

    if not events or not events[-1].content or not events[-1].content.parts:
        print("")
        exit(1)

    print("\n".join([p.text for p in events[-1].content.parts if p.text]))
