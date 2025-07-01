# Orchestratoragent 사양

## 목적

사용자 의도를 구문 분석하고 모호성을 해결하고 도메인 에이전트를 시퀀싱하며 Ask -to -Act 거버넌스를 시행합니다.

## 주요 책임

* 의도 분류 및 슬롯 충전 (LLM 또는 Regex 바로 가기).
* 종속성 그래프 계획 : 에이전트 순서를 선택하고 출력을 전달합니다.
* 충돌 해결 (예 : 겹치는 달력 이벤트).
* 사용자에게 오류 집계 및 폴백 메시징.

## 기술

|기술 |args |반환 |
|---------------- |-------------------------------- |---------- |
|`rout_task` |`작업 : String`,`Due? : Isodate` |`taskId` |
|`plan_Sequence` |`의도 : String`,`컨텍스트 '|`plan []`|
|`resolve_conflict` |`EntityType`,`후보자 []`|`결정`|

## 워크 플로 예제

```인어
시퀀스 인디 아그램
사용자->> Conversationagent : "금요일 회의를 다음 주로 이동"
ConversationAtent- >> Orchestratoragent : 구문 분석 의도
Orchestratoragent- >> 스케줄링 : 충돌을 확인하십시오
ScheduleAgent- >> Orchestratoragent : 무료 슬롯
OrchestratorAgent- >> 대화 대기 : 제안
사용자-> ConversationAgent : 승인
Orchestratoragent- >> ScheduleAgent : Create_event
```

## 성능 대상

* 의도 라우팅 대기 시간 <150ms.
* 98% 성공적인 계획 실행 수동 재정의.

## 오류 처리

* 알 수없는 의도 → ConversationAnt를 통한 설명 요청.
* 다운 스트림 에이전트 오류 → 재 시도 2 ​​× 그런 다음 요약을 사용자에게 알립니다.

## 보안 및 개인 정보

* 다운 스트림 에이전트에게 최소한의 필요한 컨텍스트 만 통과하십시오.
* 로깅하기 전에 개인적으로 민감한 데이터를 제거하십시오.
