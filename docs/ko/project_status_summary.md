# Floe AI Assistant- 프로젝트 상태 요약

## 1. 프로젝트 비전 및 목표

* 프로젝트 비전은 사용자 개인 정보 (`Docs/Product-OverView.md`에서)를 보존하면서 자연스러운 대화를 통해 일정, 작업, 커뮤니케이션 및 복지를 쉽게 조정하는 기기 AI 보조원을 만드는 것입니다.
* 핵심 값에는 "단일 자료 ▲ no re-enry"(한 번 정보 수집 및 재사용), "사전 예방 적이지만 ask-to-act"(에이전트 제안, 사용자 승인) 및 "LLM이 로컬로 운영되고 동기화가 암호화되고 Opt-in)가 포함됩니다 (Docs/Product-OverView.md`).
* 1 차 페르소나는 "바쁜 전문가"(명확한 매일 매일 브리프 및 스마트 리마인더를 찾고), "스타트 업 설립자"(위임 된 심사 및 KPI 스냅 샷 욕구) 및 "건강에 초점을 맞춘 작업자"( 'Docs/Product-Overview.md'에서 통찰력을 목표로합니다)입니다.
* 목표는 일정, 과제, 의사 소통 및 복지를 조정하기위한 기기 AI 보조원을 만드는 것입니다.

## 2. 현재 건축 상태

* Floe는 중앙 요원이 조율 한 다중 에이전트 아키텍처를 사용합니다.각 에이전트는 특정 도메인 (`docs/work_plan.md` 및`docs/excection_plan.md`)을 담당합니다.
* 핵심 구성 요소에는 다음이 포함됩니다 :`Orchestratoragent` (Routes 사용자 명령),`MemoryManagerAgent` (메모리 관리) 및 대화, 스케줄링, 작업,받은 편지함, 건강 및 통찰력 (`Docs/Work_plan.md` 및`docs/expectation_plan.md`)을위한 전문 에이전트가 포함됩니다.
* Core Technologies는 Python, OpenAi의 에이전트 SDK 및 MCP 서버 통합 (`docs/work_plan.md` 및`docs/exceusation_plan.md`)입니다.'Product-OverView.md'는 또한 전자 호스트, 다중 에이전트 추론을위한 Anthropic MCP, Llama-CPP-Python LLM 백엔드 및 로컬 벡터 스토어 (SQLITE + Chroma)를 언급합니다.
*`docs/ublemation_plan.md`는 기본 기술 사양 문서 역할을합니다.

## 3. 에이전트 구현 상태

### 3.1.오케스트라 레지 텐트
*** 역할 ** : 사용자 명령 분석 및 라우팅 (`docs/work_plan.md`,`docs/excection_plan.md`).
*** 상태 ** : MVP v1.0의 일부이므로 초기 구현이 존재할 수 있습니다 ( "Docs/Product-OverView.md`에 언급 된"오케스트레이터, 메모리 ").
*** 구현/지정된 모듈 (`ubstraction_plan.md`에서) : **
*`intent_analyzer.py`
*`Orchestrator_core.py`
*** 주요 기능 계획 ** : 자연 언어, 순차적/병렬 오케스트레이션, 컨텍스트를위한 MemoryMamerAmanagerAgent 통합, 응답 집계 (`docs/work_plan.md`,`docs/exceusation_plan.md`)의 의도 분석.

### 3.2.MemoryManagerAgent
*** 역할 ** : 장기 및 단기 메모리 관리 (`docs/work_plan.md`,`docs/excection_plan.md`).
*** 상태 ** : MVP v1.0의 일부이므로 초기 구현이 존재할 수 있습니다 ( "Docs/Product-OverView.md`에 언급 된"오케스트레이터, 메모리 ").
*** 구현/지정된 모듈 (`ubstraction_plan.md`에서) : **
*`memory_store.py`
*`memory_retriever.py`
*** 키 기능 계획 ** : 벡터화 및 저장, 시맨틱 검색, TTL/recency priorization, 자동 컨텍스트 주입 (`docs/work_plan.md`,`docs/exceusation_plan.md`).

### 3.3.대화가 젠장
*** 역할 ** : 자연어 상호 작용 및 상황 유지 보수 (`docs/work_plan.md`,`docs/exceusation_plan.md`).
*** 상태 ** :`ublestration_plan.md`에 지정된 설계.최근 계획 파일 (`Docs/Planning _*. MD`)에서는 구체적인 구현 진행 상황이 없습니다.
*** 지정된 모듈 (`ustementation_plan.md`) : **
*`input_handler.py`
*`dialogue_manager.py`
*** 주요 기능 계획 ** : 텍스트/음성 입력 처리, 대화 흐름 관리, 설명 질문, 작업 중단/재 관리, 캐시 된 의도 처리 (`docs/work_plan.md`,`docs/excection_plan.md`).

### 3.4.스케줄링
*** 역할 ** : 일정 제작, 충돌 감지 및 시간 추천 (`docs/work_plan.md`,`docs/implementation_plan.md`).
*** 상태 ** : MVP v1.0 작업 및 일정 흐름 (`Docs/Product-OverView.md`)의 일부이므로 초기 구현이있을 수 있습니다.
*** 구현/지정된 모듈 (`ubstraction_plan.md`에서) : **
*`schedule_parser.py`
*`calendar_connectors.py`
*`scheduler_core.py`
*`schedule_summary.py`
*** 주요 기능 계획 ** : NLP, Google/Microsoft 캘린더 통합, 회의 시간 권장 사항, 반복 일정 처리, 스케줄 요약 (`docs/work_plan.md`,`docs/ubledation_plan.md`)의 일정 세부 정보.

### 3.5.TaskAgent
*** 역할 ** : 작업 생성, 구조화, 우선 순위 및 알림 (`DOCS/WORK_PLAN.MD`,`DOCS/ADVEMENTATION_PLAN.MD`).
*** 상태 ** : 진행 중입니다.MVP v1.0 작업 및 일정 흐름의 일부.
*** 구현/지정된 모듈 : **
*`task_parser.py`
*`task_core.py` : 구현은 다음 즉시 단계로 계획되었습니다 (`docs/planning_20250617_050130.md` 및`docs/planning_20250617_151812.md`에 따라).
*`task_calendar_linker.py` :`ustmentation_plan.md`에 지정되었습니다.
*** 주요 기능 계획 ** : NLP에서 작업 구조화, 작업 항목 추출, 우선 순위 계산, 캘린더 차단, 자동 연결 작업 및 일정 (`docs/work_plan.md`,`docs/exceptation_plan.md`).

### 3.6.받은 편지원
*** 역할 ** : 이메일, 알림 및 외부 메시지에서 정보를 분석하고 추출합니다 (`docs/work_plan.md`,`docs/ublemation_plan.md`).
*** 상태 ** :`ublestration_plan.md`에 지정된 설계.최근 계획 파일에서는 구체적인 구현 진행 상황이 없습니다.
*** 지정된 모듈 (`ustementation_plan.md`) : **
*`email_connectors.py`
*`email_processor.py`
*** 주요 기능 계획 ** : Gmail/Outlook Integration, LLM 기반 이메일 요약, 일정 제안/요청 인식 및 ScheduleAgent로의 전달, TaskAgent에 대한 작업/첨부 파일 추출, MemoryMemanagerAgent 아카이브 (Docs/work_plan.md`,`docs/udementation_plan.md`).

### 3.7.HealthAgent
*** 역할 ** : 건강 관리 자동화 (수면,식이 요법, 운동, 스트레스) (`docs/work_plan.md`,`docs/exceusation_plan.md`).
*** 상태 ** :`ublestration_plan.md`에 지정된 설계.로드맵 v1.1 (`docs/product-overview.md`에 따라 "수면 및 활동 모듈").
*** 지정된 하위 모듈 (`roductation_plan.md '에서`work_plan.md`)를 참조하십시오. : **
*`sleepModule '(`sleep_module.py`)
*`NutritionModule '(`Nutrition_Module.py`)
*`ActivityModule '(`activity_module.py`)
*`wellnessmodule '(`wellness_module.py`)
*** Core 지정 모듈 (`ubstrantation_plan.md`에서) : **
*`health_models.py`
*`Wearable_Connectors.py`
*`health_predictor.py`
*`OverWork_analyzer.py`
*`health_reporter.py` (``집니다.
*** 주요 기능 계획 ** : 수면/식사 시간 예측/확인, 웨어러블 장치 통합, 과로 작업 탐지/알림, 주간 건강 요약 보고서 (`DOCS/WORK_PLAN.MD`,`DOCS/INDUCEATION_PLAN.MD`).

### 3.8.통찰력
*** 역할 ** : 사용자 동작 패턴 분석 및 보고서 생성 (`Docs/Work_plan.md`,`Docs/exceusation_plan.md`).
*** 상태 ** :`ublestration_plan.md`에 지정된 설계.로드맵 v1.2 (`Docs/Product-OverView.md`에 따라 "트렌드 대시 보드 및 목표").
*** 지정된 모듈 (`ustementation_plan.md`) : **
*`Insight_generator.py` (`utubleation_plan.md` 섹션 3.8, 기술 구현에 따라)
*** 주요 기능 계획 ** : 로그의 통합 분석 (일정, 작업, 수면), 생성생산성/건강 보고서, 맞춤 정기 권장 사항, 행동 개선 알림, 트렌드 시각화 (`Docs/Work_plan.md`,`Docs/excection_plan.md`).

## 4. MCP 서버 통합

*** 상태 ** : 자세한 통합 계획은`docs/exceentation_plan.md` (섹션 4)에 존재합니다.
*** 계획의 주요 측면 ** :
* API 정의 (OpenAPI, RESTFUL, 버전).
* 데이터 스키마 (Pydantic Models, JSON Interchange).
* 인증 및 인증 (OAUTH 2.0, JWTS, RBAC 에이전트; MCP를 통한 사용자 인증).
* 비동기 통신 (Rabbitmq/Kafka, Webhooks와 같은 메시지 대기열).
* 오류 처리 (표준 HTTP 코드, 일관된 오류 JSON 구조).
* API 게이트웨이 고려 사항 (예 : AWS API 게이트웨이, Kong).
* 데이터 지속성, 메시지 대기열, API 게이트웨이, IAM, 서비스 검색, 로깅/모니터링에 대한 기존 MCP 인프라 또는 일반 서비스 활용.
(모두`docs/ublementation_plan.md` 섹션 4에서).
* MCP 서버 자체의 실제 구현 상태 또는이 계획 이외의 통합 지점은 제공된 문서에 자세히 설명되어 있지 않습니다.

## 5. 테스트 전략 구현

*** 상태 ** : 포괄적 인 테스트 전략은`docs/exceentation_plan.md` (섹션 5)에 정의됩니다.
*** 전략의 주요 측면 ** :
* 단위 테스트 (`pytest`,`ittest.mock`,> 80-90% 적용 범위를 목표로합니다).
* 통합 테스트 (에이전트 대 에이전트, 에이전트-MCP, 에이전트 대 예측).
* 엔드 투 엔드 (E2E) 테스트 (완전한 사용자 시나리오 시뮬레이션, API 구동).
* 테스트 데이터 관리 (격리, 생성, 정리).
* CI/CD (GitHub Actions, Jenkins, Gitlab CI; 실패한 테스트 블록 병합/배포).
(`docs/ublementation_plan.md` 섹션 5에서 모두).
*** 현재 진행 상황 ** : 구현 된 모듈에 대해 단위 테스트가 작성되고 있습니다.`task_parser.py`는`tests/task_agent/test_task_parser.py` (`docs/planning_20250617_050130.md`에 언급 된대로)에서 단위 테스트가 있습니다.

## 6. 배포 전략 구현

*** 상태 ** : 배포 전략은`docs/exceentation_plan.md` (섹션 6)에 요약되어 있습니다.
*** 전략의 주요 측면 ** :
* 컨테이너 화 (Docker, 최적화 된 dockerfiles, 로컬 개발의 경우 Docker-compose ').
* 오케스트레이션 (Kubernetes 선호, 헬름 차트).
* 환경 전략 (Dev, Test/Staging, Prod Kubernetes 클러스터).
* Cloud-Native Preferred (AWS, Google Cloud, Azure for Managed K8S 등).
* 확장 성 (정책 에이전트, 수평 POD Autoscaler).
* 모니터링 및 경고 (Prometheus, Grafana, Alertmanager; K8S Livendes/Readiness Probes).
* 로깅 (중앙화, 구조화 된 JSON, 상관 ID).
* 구성 관리 (환경 변수, K8S 구성/비밀, Hashicorp Vault).
(모두`docs/ublementation_plan.md` 섹션 6).
*** 현재 진행 상황 ** : 전략이 문서화됩니다.계획 이외의 구체적인 구현 진행은 제공된 문서에 자세히 설명되어 있지 않습니다.

## 7. 제품 로드맵 정렬

*** MVP (v1.0) ** : "작업 및 일정 흐름, 오케스트레이터, 메모리"(`Docs/Product-OverView.md`에서).
* 'Orchestratoragent',`MemoryManagerAgent ',`ScheduleAgent'는 이에 대한 핵심이며 초기 구현이있을 수 있습니다.
*`taskAgent`는 현재 MVP와 일치하는`taking_parser.py` 구현,`task_core.py` the`the`task_parser.py ')가 진행 중입니다.
*** v1.1 Health ** : "수면 및 활동 모듈"(`Docs/Product-OverView.md`).이것은 'HealthAgent'와 일치합니다.
*** v1.2 Insights ** : "트렌드 대시 보드 및 목표"(`Docs/Product-OverView.md`).이것은 'Insightagent'와 일치합니다.

이 요약은 2025-06-17 기준으로 제공된 프로젝트 문서의 최신 정보를 기반으로합니다.
