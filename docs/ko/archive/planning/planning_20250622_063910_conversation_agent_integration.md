# 대화 에이전트 오케스트레이터 통합 ​​계획 -2025-06-22 06:39 UTC

ConversationAgent 기본 모듈의 완료를 기반으로 다음 단계는 OrchestratorEngine과 통합하여 일반적인 대화 의도를 새로운 에이전트에 의해 처리하는 것입니다.

## 목표
-`conversionagent '를 래핑하여`baseagent'인터페이스를 준수합니다.
-`general_conversation` 의도에 대해`orchestrationengine '으로이 래퍼를 등록하십시오.
- 사용 가능한 경우 등록 된 에이전트를 사용하도록 라우팅 로직을 업데이트하십시오.
-이 동작을 확인하는 단위 테스트를 제공합니다.

## 계획된 작업
1. **`chongring_agent/orchestrator_wrapper.py` **
-`baseagent`에서```````````````` " '상속든 상속.
-`name` 및`supported_intents` (`[ 'general_conversation']`)를 노출시킵니다.
-`process ()``conversation.handle_message '를 호출하고`agentresponse'를 반환합니다.
2. **`Orchestrator_agent/Orchestrator_core.py` **
- 에이전트가 의도에 등록되면 'general_conversation'에도 사용되도록`rout_request`를 수정하십시오.
- 에이전트가 등록되지 않은 경우에만 기존 동작으로의 폴백.
3. ** 테스트 **
- 대화 에이전트 래퍼를 사용하여 엔진 생성을 추가하십시오.
-`general_conversation '의도가 래퍼로 라우팅되고 오케스트레이터 응답에 래퍼의 응답이 나타나는지 확인하십시오.
4. ** 문서 **
-이 작업을 요약하고 구현 후 다음 단계를 개요하십시오.
