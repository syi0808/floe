from python_a2a import agent, skill, run_server
from lib.llm.llama_client import LlamaClient  # 제공된 LlamaClient 사용
from lib.server.llm import LlamaCppA2AServer  # 제공된 LlamaCppA2AServer 사용

llm = LlamaClient()


@agent(
    name="ScheduleAgent",
    description="일정 관리: 자연어 파싱, 충돌 검사, 시간 추천, 이벤트 생성, 요약",
    version="1.0.0",
)
class ScheduleAgent(LlamaCppA2AServer):

    def handle_task(self, task):
        # 태스크가 시작될 때 로깅
        print(f"[ScheduleAgent] Handling task {task.id} start")
        # 기본 로직으로 스킬 디스패치
        result = super().handle_task(task)
        # 태스크 처리 결과 출력
        print(
            f"[ScheduleAgent] Task {task.id} completed with status {result.status.state}"
        )
        return result

    @skill(
        name="자연어 일정 파싱",
        description="자연어로 입력된 일정을 구조화된 이벤트 데이터로 변환",
        tags=["nlp", "schedule", "parsing"],
    )
    def parse_schedule_text(self, text: str) -> dict:
        functions = [
            {
                "name": "parse_event",
                "description": "Convert text to event JSON",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "start_time": {"type": "string"},
                        "end_time": {"type": "string"},
                        "location": {"type": "string"},
                        "participants": {"type": "array", "items": {"type": "string"}},
                    },
                    "required": ["start_time", "end_time"],
                },
            }
        ]
        resp = llm.chat(
            messages=[{"role": "user", "content": text}],
            functions=functions,
            function_call="auto",
        )
        return resp["choices"][0]["message"]["function_call"]["arguments"]

    @skill(
        name="일정 충돌 감지",
        description="신규 이벤트와 캘린더 간 시간 충돌 여부 확인",
        tags=["calendar", "conflict"],
    )
    def check_conflict(self, event: dict) -> dict:
        conflict = False
        return {"conflict": conflict}

    @skill(
        name="시간 추천",
        description="참여자 가용 시간을 기반으로 회의 시간 추천",
        tags=["availability", "recommendation"],
    )
    def suggest_time_slot(self, participants: list[str]) -> dict:
        suggestions = ["2025-05-02T10:00", "2025-05-02T14:00"]
        return {"suggestions": suggestions}

    @skill(
        name="이벤트 생성",
        description="구조화된 이벤트 데이터를 캘린더에 등록",
        tags=["calendar", "event", "integration"],
    )
    def create_event(self, event: dict) -> dict:
        return {"status": "created", "event": event}

    @skill(
        name="일정 요약",
        description="주간/일일 일정을 간결히 요약하여 제공",
        tags=["summary", "overview"],
    )
    def summarize_schedule(self, timeframe: str) -> dict:
        summary = f"{timeframe}에 총 5개의 일정이 있습니다."
        return {"summary": summary}


if __name__ == "__main__":
    agent = ScheduleAgent()
    run_server(agent, host="0.0.0.0", port=7002)
