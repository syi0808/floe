# Floe AI Assistant- 나머지 작업 계획

## 1. 소개

*이 문서는 Floe AI 조수의 나머지 작업을 간략하게 설명합니다.이 프로젝트는`DOCS/Project_Status_Summary.md`, 전체 프로젝트 목표 및`DOCS/INDOIME_PLAN.MD` 및`DOCS/Product-OverView.md '에 지정된 자세한 계획에 문서화 된 현재 프로젝트 상태를 기반으로합니다.목표는 후속 개발 단계를 완료하기위한 명확한 로드맵을 제공하는 것입니다.

## 2. 즉시 다음 작업

*** 완료`taskAgent '구현 : **
* 'task_agent/task_core.py`cly': 여기에는 'taskitem'pydantic 모델을 정의하고, CRUD 구현 (생성, 읽기, 업데이트, 삭제) 작업에 대한 작업 구현, 작업 우선 순위를위한 기본 논리 개발 및 향후 알림 기능을위한 자리 표시자를 포함합니다.이는`docs/ubstance_plan.md` (섹션 3.5.2) 및`docs/project_status_summary.md`의 현재 상태와 일치합니다.
*``task_agent/task_calendar_linker.py`를 구현하십시오 :이 모듈은 캘린더 차단 통합을 담당하므로`docs/excleation_plan.md` (섹션 3.5.3)에 자세히 설명 된대로 시간 할당을 위해`scheduleagent`와 연결할 수 있습니다.
* 단위 테스트 개발 :`task_core.py` 및`task_calendar_linker.py` 내의 모든 기능에 대한 포괄적 인 단위 테스트를 만듭니다.
* 통합 테스트 수행 :`taskAgent '에 대한 통합 테스트를 수행하여`scheduleAgent'(캘린더 링크) 및 'MemoryManagerAgent'(작업 관련 데이터 저장 및 검색)로 올바르게 작동하는지 확인하십시오.

## 3. 나머지 에이전트의 구현

다음 에이전트는 Floe의 기능을 확장하는 데 핵심입니다.그들의 구현은`docs/ubstract_plan.md`의 사양을 따릅니다.

### 3.1.대화가 젠장
*** 목표 ** : 자연어 상호 작용을위한 핵심 기능을 구현하여 원활하고 상황을 인식하는 사용자 대화를 가능하게합니다.
*** 구현 할 모듈 (`utubleation_plan.md` 섹션 3.3) : **
*`input_handler.py` : 처음에는 텍스트 기반 입력의 강력한 처리에 중점을 둡니다.음성 입력 기능은 향후 향상으로 간주됩니다.
*`Dialogue_Manager.py`
*** 주요 통합 ** :`MemoryManageRagent` (대화 내용과 컨텍스트를 저장하고 검색하기 위해), 'OrchestratorAgent'(사용자 쿼리에서 의도 및 엔터티 추출을 얻기 위해).
*** 테스트 ** :`input_handler.py` 및`dialogue_manager.py`에 대한 단위 테스트를 구현하십시오.'Orchestratoragent'및 'MemoryManagerAgent'와의 상호 작용을 확인하기 위해 통합 테스트를 수행합니다.

### 3.2.받은 편지원
*** 목표 ** : 이메일 및 기타 외부 메시지 분석, 관련 정보 추출 및 적절하게 라우팅하는 기능을 구현합니다.
*** 구현 할 모듈 (`utubleation_plan.md '섹션 3.6) : **
*`email_connectors.py`
* 'email_processor.py`
*** 주요 통합 ** :`scheduleAgent` (일정 제안 제안),`taskAgent '(식별 된 작업),`MemoryManagerAgent'(중요한 정보/첨부 파일 보관) 및 MCP (이메일 서비스에 연결하고 잠재적으로 추출한 데이터를 사용하는 경우).
*** 테스트 ** : 커넥터 로직 및 이메일 처리 기능에 대한 단위 테스트를 만듭니다.라이브 이메일 서비스 (전용 테스트 계정 사용) 및 'inboxagent'에서 데이터를 수신하는 에이전트와 통합 테스트를 수행하십시오.

### 3.3.HealthAgent (로드맵 v1.1)
*** 목표 ** : 스위트를 구현하십시오수면, 영양, 활동 및 일반적인 건강과 같은 측면을 다루는 개인 건강 관리 자동화 기능.이것은`docs/product-overview.md`의 로드맵 v1.1과 일치합니다.
*** 구현할 핵심 모듈 (`utubleation_plan.md '섹션 3.7) : **
*`health_models.py`
*`Wearable_connectors.py`
*`health_predictor.py`
*`OverWork_analyzer.py`
*`health_reporter.py` : 주간 건강 요약 및 통찰력을 생성하는 도구를 개발하십시오.
*** 구현 할 하위 모듈 (`roduction_plan.md` 섹션 3.7에서`work_plan.md`를 참조) : **
*`sleep_module.py` ( 'sleepModule') : 수면 벌목, 수면 부족 계산 및 회복 제안 생성을 구현하십시오.
*`Nutrition_Module.py` ( 'NutritionModule') : 식사 벌목, 영양소 추적 (설명에서 추정에 LLM을 사용) 및 식사 알림을 활성화합니다.
*`activity_module.py` (`ActivityModule ') : 활동 로깅을 구현하고 운동 루틴에 대한 제안을 제공합니다.
*`wellness_module.py` (`wellnessmodule ') : 스트레스/기분의 로깅을 허용하고 패턴을 분석하며 복구 루틴을 권장합니다.
*** 주요 통합 ** :`ScheduleAgent` (예측을위한 상황에 맞는 데이터),`MemoryManagerAgent '(건강 로그 및 사용자 선호도 저장), MCP (건강 추적기에서 데이터를 수집하고 건강 관련 알림을 파견).
*** 테스트 ** : 모든 개별 모듈에 대한 단위 테스트를 개발합니다.웨어러블 장치 또는 사전 정의 된 테스트 데이터 세트 용 에뮬레이터를 사용하여 통합 테스트를 수행합니다.

### 3.4.InsightAgent (로드맵 v1.2)
*** 목표 ** : 다양한 도메인에서 사용자 행동 패턴을 분석하고 보고서를 생성하며 생산성과 복지 향상을위한 실행 가능한 통찰력을 제공하는 기능을 구현합니다.이것은`docs/product-overview.md`의 로드맵 v1.2와 일치합니다.
*** 구현 할 모듈 (`utubleation_plan.md` 섹션 3.8) : **
* 'Insight_generator.py` :이 모듈은 일일 및 주간 통찰력 보고서를 생성하고, 개인화 된 일상적인 권장 사항을 제공하고, 트렌드 시각화를위한 데이터를 작성하고 (예 : 고객 측면 렌더링을위한 JSON 형식), 사용자 정의 목표를 향한 진행 상황 추적, 다양한 기간 동안 측정 항목 비교를 담당합니다.
*** 데이터 입력 ** :`InsightAgent '는`scheduleAgent`,`taskAgent`,`healthAgent'및`memoryManagerAgent '에서 집계 된 데이터를 소비합니다.
*** 주요 통합 ** : MCP (다른 에이전트의 데이터 집계를 용이하게하고 새로운 보고서 나 통찰력을 사용할 수있을 때 알림을 보내기 위해).
*** 테스트 ** : 통찰력 생성 로직에 대한 단위 테스트를 만듭니다.데이터 집계 파이프 라인 및 생성 된 보고서의 정확도에 중점을 둔 통합 테스트를 수행하십시오.

## 4. 추가 MCP 서버 통합 개발

*** 목표 ** : 개발 및 정제를 겪으므로 각 에이전트 모듈에 대해`docs/extern_plan.md` (섹션 4)에 정의 된 MCP 서버 통합 지점을 점차 구현하고 철저히 테스트합니다.
*** 주요 작업 ** :
* 에이전트가 통신 할 MCP 서버 엔드 포인트를 개발 및/또는 구성합니다 (예 : 사용자 쿼리 수신에 대한`post/mcp/commands`,`get/mcp/memories/{user_id}/search 'for Memory Resprieval).
* MCP API와 상호 작용하기 위해 각 에이전트 내에서 클라이언트 측 논리를 구현하십시오 (예 :`Post /MCP /Intoke_Service`는 에이전트가 MCP를 통해 다른 서비스를 트리거 해야하는 경우,`post /mcp /allifications`가 경고 또는 업데이트를 보내기 위해).
* 정의 된 모든 보안 인증 (예 : Oauth 2.0, Agent-MCP의 JWT) 및 인증 메커니즘이 올바르게 구현되고 시행되어 있는지 확인하십시오.
* 메시지 대기열 (예 : RabbitMQ, Kafka를 통해 MCP를 통한 Kafka) 또는 Webhooks와 같은 비동기 통신 채널을 설정, 구성 및 테스트하여 특히 'InboxAgent'(새로운 이메일 알림) 또는 시스템 전체에 알림을 발송하기 위해 설정합니다.
* 오류 처리 메커니즘을 엄격하게 확인하고 에이전트 MCP 통신에 대한 탄력성 패턴 (예 : 검색, 폴백)을 구현하십시오.

## 5. 광범위한 테스트 전략 실행

*** 목표 ** : 모든 개발 단계에서`docs/exceentation_plan.md` (섹션 5)에 요약 된 포괄적 인 테스트 전략을 체계적으로 실행하고 확장합니다.
*** 주요 작업 ** :
*** 단위 테스트 계속 ** : 각 에이전트 내의 모든 새롭고 수정 된 코드에 대해 높은 단위 테스트 커버리지 (중요 모듈의 경우> 80-90%를 목표)를 유지하고 시행합니다.
*** 통합 테스트 개발 ** : 개별 에이전트 및 모듈이 완료됨에 따라 다음에 중점을 둔 통합 테스트를 개발하고 실행합니다.
* 직접 에이전트-에이전트 상호 작용 (예 : 'Orchestratoragent'에서 'ScheduleAgent').
* Agent-to-MCP 상호 작용 (MCP가 에이전트 엔드 포인트를 호출하는 경우 에이전트-클라이언트 및 에이전트-서버).
* 에이전트 간 서비스 상호 작용 (예 :``ScheduleAgent '', Google/Microsoft Calendar APIS, 'inboxagent'가 이메일 제공 업체 API와 함께).
*** en
*** CI/CD 파이프 라인 향상 ** : 모든 유형의 자동 테스트 (단위, 통합, E2E)를 CI/CD 파이프 라인에 지속적으로 통합합니다.커밋/합병에 대한 자동 실행 및 테스트 결과에 대한 명확한보고를 확인하십시오.
*** 테스트 데이터 관리 ** : 신뢰할 수 있고 반복 가능한 테스트를 보장하기 위해 테스트 데이터를 생성, 관리, 분리 및 정리하기위한 강력한 절차를 설정하고 개선합니다.

## 6. 배포를 향한 단계

*** 목표 ** :`docs/exceentation_plan.md` (섹션 6)에 자세히 설명 된 전략에 따라 플로어 구성 요소의 배포를 점차적으로 준비하고 실행합니다.
*** 주요 작업 ** :
*** 컨테이너 화 ** : 모든 에이전트 및 모든 MCP 관련 구성 요소 (별도의 서비스로 관리되는 경우)가 잘 정의되고 최적화 된`dockerfile`s가 있는지 확인하십시오.
*** 지역 오케스트레이션 ** : 다중 에이전트 시스템의 현지 개발 및 테스트를 용이하게하기 위해 'Docker-Compose'구성 유지 관리 및 개선.
*** Kubernetes 준비 ** :
* 각 배포 가능한 에이전트/서비스에 대한 Kubernetes Manifests (배포, 서비스, 구성, 비밀 등)를 개발, 테스트 및 개선합니다.
* Kubernetes에 Floe 구성 요소 배치를 포장하고 관리하기위한 Helm 차트를 평가하고 잠재적으로 구현합니다.
*** 환경 설정 ** : 필요한 테스트/스테이징 및 생산 환경을 계획하고 구성하여 클라우드 네이티브 솔루션 (AWS EKS, Google GKE, Azure AKS)의 우선 순위를 정합니다.
*** 모니터링 및 로깅 ** : 에이전트가 이러한 환경에 배치 될 때 모니터링 도구 (예 : 메트릭의 프로 메테우스, 대시 보드 용 그라파나) 및 중앙 집중식 로깅 솔루션 (예 : ELK 스택, 로키 또는 클라우드 제공자 등장품)을 구현 및 구성합니다.
*** 구성 및 비밀 관리 ** : Application Configuration이 주입되는 방법 (구성을 통해)과 비밀 (API 키, 자격 증명)이 Kubernetes 내에서 Kubernetes 비밀 또는 Hashicorp Vault와 같은 전용 솔루션을 통해 비밀 (API 키, 자격 증명)이 어떻게 관리되는지 표준화하십시오.

## 7. 제품 로드맵과 정렬

*** v1.0 (mvp) ** : 즉각적인 우선 순위는`taskagent '를 완료하고 있습니다.여기에는`task_core.py`와`task_calendar_linker.py`를 마무리하고 'Orchestratoragent',`MemoryManageRagent '및`scheduleAgent'의 강력한 기능을 보장하는 것이 포함됩니다.이 단계에는 필수가 포함됩니다`docs/product-overview.md`에 따라 이러한 핵심 구성 요소에 대한 Ary MCP 통합 및 철저한 테스트.
*** v1.1 Health ** : MVP 구성 요소가 안정적이고 검증되면 개발은 '핵심 모듈 및 하위 모듈 (수면, 영양, 활동, 건강)에 중점을 둔'HealthAgent '로 전환됩니다.
*** v1.2 Insights ** : 'HealthAgent'의 성공적인 구현 및 테스트에 따라 초점은 'Docs/Product-Overview.md`에 따라'InsightAgent '로 이동하여 추세 분석, 보고서 생성 및 목표 추적을 가능하게합니다.
*`ConversationAgent '및`InboxAgent'는 기본 상호 작용 및 정보 수집 기능을 제공합니다.그들의 개발 및 통합은 모든 로드맵 버전의 기능을 지원하기 위해 우선 순위를 정해야하며 필요에 따라 병렬 또는 점진적으로 개발할 수 있습니다.

이 계획은 개발 진행 상황에 따라 업데이트 및 개선에 따라 살아있는 문서가 될 것입니다. 새로운 도전이 식별되고 추가 정보가 제공됩니다.
