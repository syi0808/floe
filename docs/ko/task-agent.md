# 태스크 에이전트 사양

## 목적

사용자 의도 및 추출 된 항목을 실행 가능한 작업으로 변환하고 수명주기를 관리하며 시간 블로킹을 위해 ScheduleAgent와 동기화합니다.

## 기술

|기술 |args |반환 |
|--------------- |-------------------------------- |--------- |
|`add_task` |`title`,`due?`,`우선 순위?`,`reminderoffset?`|`taskId` |
|`update_task` |`taskId`,`patch` (`reminderOffset?`) |`작업`|
|`schedule_block` |`taskId`,`duration`,`창?`|`eventId` |
|`snooz |`taskId`,`toall` |`작업`|

## 우선 순위 공식

```
score = (중요도 * 0.5) + (긴급 성 * 0.3) + (Trout_Inverted * 0.2)
```

## 데이터 모델

* 'calendar_event_id`를 통해 이벤트에 연결된 sqlite에 저장된 작업.
* 하위 작업, 태그 및 알림 시간 (`reminder_time_utc`)를 지원합니다.

## 예제 흐름

1. InboxAgent는 "금요일까지 검토하십시오"→ TaskAgent`add_task '라는 문구를 감지합니다.
2. TASKAGENT`Schedule_Block` 전기 전 90 분.
3. TaskAgent는 'REMANDEROFFSET'이 지정되지 않는 한 24 시간 전에 알림을 예약합니다.

## kpis

* 작업의 ≥90%가 마감일이 있습니다.
* 스누즈 된 작업은 정시에 재 포장되었습니다.
