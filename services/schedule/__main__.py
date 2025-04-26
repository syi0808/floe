from common.server import A2AServer
from common.types import AgentCard, AgentCapabilities, AgentSkill, MissingAPIKeyError
from common.agent.agent_task_manager import AgentTaskManager
from services.schedule.agent import ScheduleAgent
import click
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


@click.command()
@click.option("--host", default="localhost")
@click.option("--port", default=10001)
def main(host, port):
    try:
        # ScheduleAgent 용 능력 설정 (streaming 활성화)
        capabilities = AgentCapabilities(streaming=True)
        # Schedule 관련 스킬 정의
        skills = [
            AgentSkill(
                id="parse_schedule_text",
                name="자연어 일정 파싱",
                description="자연어로 입력된 일정을 시간, 장소, 참여자 등의 구조화된 데이터로 변환합니다.",
                tags=["schedule", "nlp"],
                examples=["다음 주 화요일 오후 3시에 회의 잡아줘"],
            ),
            AgentSkill(
                id="check_conflict",
                name="일정 충돌 감지",
                description="새 일정이 기존 일정과 겹치는지 확인합니다.",
                tags=["conflict", "calendar"],
                examples=["이 시간에 다른 약속 있나요?"],
            ),
            AgentSkill(
                id="suggest_time_slot",
                name="시간 추천",
                description="참여자 가용 시간 기반으로 추천 가능한 시간대를 제안합니다.",
                tags=["recommendation", "availability"],
                examples=["다음 가능한 1시간 블록을 추천해줘"],
            ),
            AgentSkill(
                id="create_event",
                name="일정 생성",
                description="구조화된 일정 데이터를 캘린더에 생성합니다.",
                tags=["calendar", "event"],
                examples=["회의를 캘린더에 추가해줘"],
            ),
            AgentSkill(
                id="summarize_schedule",
                name="일정 요약",
                description="등록된 일정을 요약하여 사용자에게 제공합니다.",
                tags=["summary", "overview"],
                examples=["이번 주 일정을 정리해줘"],
            ),
        ]
        # AgentCard 생성
        agent_card = AgentCard(
            name="Schedule Agent",
            description="일정 생성, 충돌 감지, 시간 추천, 일정 요약 기능을 제공하는 에이전트입니다.",
            url=f"http://{host}:{port}/",
            version="1.0.0",
            defaultInputModes=ScheduleAgent.SUPPORTED_CONTENT_TYPES,
            defaultOutputModes=ScheduleAgent.SUPPORTED_CONTENT_TYPES,
            capabilities=capabilities,
            skills=skills,
        )
        # 서버 초기화 및 실행
        server = A2AServer(
            agent_card=agent_card,
            task_manager=AgentTaskManager(agent=ScheduleAgent()),
            host=host,
            port=port,
        )
        server.start()
    except MissingAPIKeyError as e:
        logger.error(f"Error: {e}")
        exit(1)
    except Exception as e:
        logger.error(f"An error occurred during server startup: {e}")
        exit(1)


if __name__ == "__main__":
    main()
