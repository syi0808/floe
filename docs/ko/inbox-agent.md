# inboxagent 사양

## 목적

전자 메일/알림을 구문 분석하고 실행 가능한 데이터를 추출하고 적절한 에이전트 (일정, 작업, 메모리)에 대표합니다.

## 지원 채널

* Gmail API (REST + PUB/SUB)
* 일반 IMAP
* Slacking Incoming Webhooks (미래)

## 기술

|기술 |args |반환 |
|---------------- |----------------------- |----------- |
|`fetch_email` |`query`,`한계 '|`스레드 []`|
|`summarize_thread` |`threadid` |`요약`|
|`watch_thread` |`stressid`,`조건` |`WatchId` |

## 추출 휴리스틱

* Chrono를 통한 날짜 감지.
* 액션 동사 ( "검토", "승인", "일정") → TaskAgent.
* RSVP 초대 → 스케줄링.

## 개인 정보 보호 제어

* 전송이 필요하지 않는 한 Oauth Scopes는 읽기로 제한됩니다.
* MemoryManager에 저장하기 전에 pii가 편집되었습니다.

## 예

이메일 제목 : "5/20에 의한 제안 검토"→ TaskAgent`add_task ' "검토 제안서"2025-05-20.
