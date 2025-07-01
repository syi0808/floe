# ActivityModuleAgent 사양

## 목적

운동과 일일 운동을 추적하고 불균형을 감지하며 운동 루틴이나 휴식을 제안하십시오.

## 데이터 입력

* 계단, 활성 칼로리, 심장 영역 구역.
* 운동 메타 데이터 (유형, 지속 시간, 강도).

## 기술

|기술 |args |반환 |
|---------------- |---------------------------------- |----------- |
|`log_workout` |`Activity ',`Derpation',`강도 '|`운동`|
|`Analyze_activity` |`Window` |`메트릭`|
|`proply_exercise` |`Goal ',`제약?`|`plan` |

## 불균형 탐지 규칙

* <4,000 단계 평균 3days → 프롬프트 라이트 워크.
* 휴식없이 3 주 이상의 HIIT 세션 → 복구 제안을 제안하십시오.

## 예제 출력

```JSONC
{
"Weekly_Summary": {
"Total_steps": 52000,
"vo2max_trend": "+1.2",
"복구 _days": 1
}
}
```
