# HealthAgent 사양

## 목적

생리 학적 및 라이프 스타일 데이터를 집계하고, 복지를 모니터링하며, 회복 또는 예방 조치를 제안합니다.

## 데이터 소스

* 지역 교량을 통한 Apple Healthkit
* Google Fit Rest
* ConversationAgent의 수동 로그

## 하위 모듈 에이전트

|모듈 |초점 |주요 지표 |
|---------------------- |----------------------------- |---------------------------- |
|** sleepModuleAgent ** |수면 단계 및 회복 |수면 \ _score, 수면 \ _debt |
|** ActivityModuleAgent ** |운동 및 운동 |단계 \ _count, 훈련 \ _load |
|** NutritionModuleAgent ** |식사 및 매크로 |kcal \ _intake, 단백질 \ _g |
|** WellnessModuleAgent ** |스트레스 및 주관적인 웰빙 |HRV, MOOD \ _Score |

## 기술

|기술 |args |반환 |
|---------------- |------------------------- |------------ |
|`log_metric` |`type`,`value ',`timestamp` |`eptyId` |
|`detect_overload` |`Window` |`Status` |
|`제안 _break` |`context` |`제안 '|
|`aggregate_daily` |`날짜 |`요약`|

## 웰니스 규칙

* 수면 부채> 90 분 → 회복 시간을 차단하도록 ScheduleAgent에 알립니다.
* HRV 드롭> 20 % 및 높은 워크로드 → 가벼운 날을 제안하십시오.
* 칼로리 결함> 500 kcal 3 일 연속 → 영양 계획을 권장합니다.

## 개인 정보 보호 및 동의

* 각 데이터 소스에 대한 명시 적 옵트 인.
* 데이터가 암호화 된 데이터;원시 유지 30 일, 집계 영원히.
