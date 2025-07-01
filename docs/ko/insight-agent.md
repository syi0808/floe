# InsightAgent 사양

## 목적

사용자가 시간이 지남에 따라 생산성, 복지 및 트렌드를 이해하는 데 도움이되는 교차 분석 및 다이제스트 보고서를 생성합니다.

## 데이터 입력

* 일정 및 작업 에이전트의 집계 이벤트/작업 통계.
* MemoryManager 집계 카운트 (작업 완료, 연기).
* HealthAgent KPI (수면 \ _score, 활동 부하).

## 기술

|기술 |args |반환 |
|---------------- |---------------------- |---------- |
|`Generate_Report` |`기간 ','초점?`|`Markdown` |
|`generate_daily_report` |`초점 ?`` |`Markdown` |
|`generate_weekly_report` |`초점 ?`` |`Markdown` |
|`compare_period` |`metric`,`from`,`to` |`diff` |
|`GOOR_PROGRESS` |`GoalId` |`Status` |

## 보고서 템플릿

*** Daily Brief ** - 의제, 우선 순위 작업, 복구 팁.
*** 주간 검토 ** - 성공, 슬립, 권장 초점.

## 시각화

* 프론트 엔드 패널을 통해 Rechart를 사용합니다.InsightAgent는 클라이언트 렌더링을 위해 JSON 사양을 반환합니다.

## kpis

* 소화 생성 <2s.
* 사용자가 허용하는 제안 된 초점 영역의 70%.

## 예제 전화

```JSONC
{
"에이전트": "Insightagent",
"기술": "Generate_Report",
"args": { "기간": "2025 -w20"}
}
```

편의를 위해 전용 도우미에게도 전화 할 수도 있습니다.

```Python
gen = insightgenerator ()
gen.generate_daily_report (user_id, data, mcp_client = client)
gen.generate_weekly_report (user_id, data, mcp_client = client)
```
