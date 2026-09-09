# Vertical Slice Delivery

> Status: Accepted delivery approach; S1/S3 in progress, later slices planned
>
> Date: 2026-09-04
>
> Decision: [ADR 0006](../../decisions/0006-slice-driven-delivery.md), amended by
> [ADR 0012](../../decisions/0012-memory-and-expert-first-slices.md) and
> [ADR 0013](../../decisions/0013-conversational-agent-learning-and-voice-sequence.md),
> with S4 scope amended by [ADR 0014](../../decisions/0014-s4-connected-agent-sources.md)
> and [ADR 0015](../../decisions/0015-s4-privacy-aware-inference.md)

## 목적과 문서 역할

Phase는 제품 범위 지도, slice는 구현·검증·인수 단위다. 모든 Phase를 조금씩
구현하는 대신 하나의 사용자 시나리오를 실제 경계 전체에 연결한다.

- 이 문서: slice 범위, 의존성, 인수 조건, 테스트와 운영 규칙.
- [PROGRESS.md](../../../PROGRESS.md): 현재 상태와 검증 근거의 단일 기록.
- [Roadmap](../00-overview/roadmap.md): 장기 기능 범위와 미검증 영역.
- ADR: 진행 방식과 아키텍처 결정의 변경 이유.

## 중심 시나리오

> 사용자가 Floe와 대화한다. Manager가 Schedule Expert와 도구를 사용해 오늘
> 일정을 이해하고 근거 있는 작업을 제안한다. 대화와 결과에서 검토 가능한 Memory와
> Playbook 개선이 쌓이며, 같은 세션을 이후 voice와 wake-up으로 호출한다.

```text
Calendar Connector → 정규화·출처·Person-scoped store
                                      ↓
Chat → Manager Agent Loop → bounded Views → Schedule Expert / Tools
                                      ↓
Session Evidence → Memory / Playbook Candidate → Review → Confirmed Knowledge
                                      ↓
Voice Session → Local Wake-up ────────┘
→ Manager → Action Proposal
→ App 근거 표시·명시적 승인
→ Policy → Validation → Permission Check → Deterministic Executor
→ Calendar Connector → 재수집 → Day Canvas 갱신
```

첫 루프는 macOS, 한 Person, Calendar connector 하나의 전체 캘린더와
일정 생성 action 하나로 제한한다. Flutter는 화면과 승인 입력을 담당하고
canonical 변경은 Rust typed command를 거친다. S4는 먼저 chat, Agent loop와
Expert host를 연결한다. S5는 그 대화와 결과를 source-backed Memory 및
procedural Playbook 후보로 컴파일한다. Expert는 제한된 view와 capability를 사용하며
connector 자격증명이나 DB에 직접 접근하지 않는다.

사용자와 대화하고 Expert 결과를 취합하는 Manager agent와 OS lifecycle을 담당하는
Device Agent를 구분한다. S4는 전자의 orchestration contract를, S7은 후자의
local wake lifecycle을 검증한다. S1–S6는 앱 실행 중 동작해도 된다.

## S1 — Connected Calendar Read

**사용자 결과:** macOS Calendar에서 포함한 캘린더의 일정이 하나의 Day Canvas에
통합 표시된다. 타임라인에서 출처를 전환하지 않으며 각 일정의 출처는 유지한다.

2026-09-04 사용자 피드백에 따라 단일 캘린더 선택 범위를 변경했다.
2026-09-05 결정: 부분 선택은 명시한 목록을 유지하고, 전체 모드만 새 캘린더를
다음 새로고침/날짜 읽기에 자동 포함한다. 기존 연결은 부분 선택으로 유지한다.
계정·캘린더 목록과 모드는 연결 관리에서 확인한다. 일부 캘린더 읽기 실패는 다른 캘린더의
성공 결과를 막지 않으며 실패한 출처의 캐시는 보존한다. 자세한 계약은
[ADR 0008](../../decisions/0008-unified-calendar-read.md)을 따른다.
프로토타입 UI 이후 native 두 모드와 출처별 복구를 구현했으며 실기기 acceptance는 남아 있다.

**의존성:** 기존 Flutter ↔ Rust ↔ Turso 기반.

macOS EventKit/OS Calendar를 우선 검토하되 provider는 아직 확정하지 않는다.
실제 읽기와 S3의 생성 권한을 짧은 PoC로 확인하고 선택 이유를 기록한다.
다른 provider가 필요하면 native Rust/Go 실행 경계를 유지하며 계획을 갱신한다.
S1 자체는 읽기 전용으로 동작하며 쓰기 권한을 미리 요구하지 않는다.
단, 2026-09-04 사용자 승인에 따라 읽기에도 전체 접근이 필요한 EventKit의
OS 권한은 예외로 허용한다. 앱의 외부 쓰기 기능은 포함하지 않는다.
선택 근거와 미검증 PoC는 [ADR 0007](../../decisions/0007-eventkit-calendar-read.md)을 따른다.

### Acceptance criteria

- **S1-A1:** 실제 provider 권한 요청·전체 캘린더 연결·목록 확인·문제 상태 표시가 앱에서 가능하며,
  거절 또는 권한 철회 시 typed error와 재연결 경로를 제공한다.
- **S1-A2:** 전체 연결 캘린더의 선택한 날짜 범위 일정이 stable external ID, connection/Person,
  revision 또는 동등한 변경 식별자, provenance와 함께 저장·표시된다.
  calendar ID/account 출처를 구분하고 all-day와 timezone 경계의 표시를 검증한다.
- **S1-A3:** 재수집해도 중복되지 않고 외부 수정·삭제가 반영된다. 제한된
  조회 범위 밖이나 실패한 캘린더의 항목을 삭제된 것으로 잘못 처리하지 않는다.
  새 캘린더 포함, 일부 출처 실패, 사라진 캘린더의 캐시 보존도 검증한다.
- **S1-A4:** 앱/core 재시작 후 일정이 유지된다. 수집 실패 시 마지막 데이터와
  stale/error 상태를 표시하고 재시도할 수 있다.

**제외:** LLM, 외부 쓰기, 다중 connector, 범용 ConnectorSpec 엔진.

## S3 — Approved Calendar Action

**사용자 결과:** 제안의 상세 내용을 승인하면 집중 일정 하나가 외부에 생성된다.

**의존성:** S1 Verified 이상, 선택 provider의 생성 capability 검증.

### Acceptance criteria

- **S3-A1:** 앱에서 대상 캘린더·제목·시작/종료·timezone을 확인하고 승인 또는
  거절한다. 승인 전이나 거절 후에는 외부 변경이 없다.
- **S3-A2:** 실행 직전에 Policy, Person, capability, 권한, 현재 일정과 제안의
  유효성을 재검증한다. 오래된 제안이나 충돌은 차단하고 재제안/재승인한다.
- **S3-A3:** 중복 클릭·재시도·재시작으로 중복 일정이 생기지 않는다. durable
  실행 ID와 상태를 보존하고 결과가 불명확하면 조회·대조 후 복구하며 맹목적으로
  재실행하지 않는다. provider 제약으로 안전한 복구가 불가능하면 사용자 확인을 요구한다.
- **S3-A4:** 실제 provider에 생성된 일정이 재수집되어 Day Canvas에 나타나고
  proposal → 승인 → 실행 결과 → external ID를 추적할 수 있다.
- **S3-A5:** 쓰기 권한 거절/철회, provider timeout, 부분 실패 시 오류와 복구
  동작을 검증한다. 모델은 executor를 우회해 외부 API를 호출할 수 없다.

**제외:** 일정 이동·삭제, 메일 전송, 자동 승인, 범용 workflow 엔진.

## S4 — Conversational Connected Agent and Expert Foundation

**사용자 결과:** 사용자가 Day Canvas의 assistant panel에서 Floe와 여러 turn을
대화한다. Manager는 Calendar, Gmail과 기기에서 허용된 attention/health context를
Contacts, 다음 일정의 위치·ETA·날씨와 함께 전문 Expert를 통해 종합해 오늘
일정 질문에 답하거나 적절한 일정 작업을 제안한다. 사용자는 각 source와 실행 과정을
확인·중단하며 나중에 같은 대화를 재개한다.

**의존성:** S3 Accepted, S1 calendar view, P0-I Agent/Expert contract harness,
P0-F의 session store at-rest/key boundary, P0-C Gmail, P0-K Apple context와
P0-L privacy-aware inference gate.
sync/account server나 resident Device Agent 없이 macOS 앱과 local Agent host에서
먼저 검증한다. model runner가 기기 밖에 있으면 inference class와 전송 동의를 따른다.

첫 범위는 text chat, 한 Person, built-in Schedule/Communication/Health Expert의
최소 projection, deterministic declarative fixture Expert와 Calendar read/create,
Gmail read/search, Contacts identity, location/ETA/weather와 device attention/health
read capability로 제한한다. source 선정 근거는
[Assistant Context Portfolio](../05-integrations/assistant-context-portfolio.md), runtime 계약은
[Agent Runtime and Governed Learning](../03-intelligence/agent-runtime-and-learning.md)을
따른다.

### Acceptance criteria

아래 기준의 `ExpertInvocation`/`ExpertResult`와 Manager-visible Expert call은 S4에서
구현된 migration baseline이다. 후속 설계 목표는
[ADR 0018](../../decisions/0018-manager-expert-a2a-delegation.md)의 in-process
A2A-aligned Message/Task/Artifact contract다.

- **S4-A1:** 새 chat을 시작하고 streaming 응답, stop, retry, 오류를 다룬다. 완결된
  User/Assistant/Tool/Expert event만 Person-scoped session에 저장하며 앱/core 재시작
  후 순서와 상태를 보존해 재개할 수 있다.
- **S4-A2:** Flutter, fixture와 model adapter가 같은 versioned AgentCommand/Event
  contract를 사용한다. stable/scoped/contextual/conversation/runtime context layer와
  내부 message/tool-call representation은 provider나 UI 구현에 종속되지 않는다.
- **S4-A3:** registry가 Tool과 Expert package/version, installation, Person assignment,
  enablement와 private state를 관리한다. built-in Schedule Expert와 declarative
  fixture가 동일한 bounded ExpertInvocation/ExpertResult contract로 실행된다.
- **S4-A4:** Expert는 granted Timeline view와 capabilities만 사용하고 DB, credential,
  raw source에 접근하지 않는다. structured insight/proposal만 Manager에게 반환하며
  Calendar 변경은 반드시 S3 Review/Policy/Validation/Executor를 재사용한다.
- **S4-A5:** 모든 model/tool/Expert call은 cancellation, deadline, iteration/token/cost,
  output과 반복-call stall budget을 지킨다. 실패 격리, trace/replay, stale context,
  prompt-injection fixture와 실제 모델 대화 세트를 통과한다. 실제 개인 대화는
  session at-rest/key-unavailable gate를 통과한 build에서만 dogfood한다.

### Model and privacy acceptance

- **S4-M1 — Common model contract:** deterministic fixture, 하나의 실제 device-local
  generative adapter와 하나의 supported remote adapter가 같은 bounded structured
  request/result/error contract를 사용한다. local adapter는 지원 기기의 Apple
  Foundation Models on-device profile을 우선하되 native packaged sLLM으로 대체할 수
  있다. Private Cloud Compute/server profile은 local acceptance로 세지 않는다.
- **S4-M2 — Codex authentication gate:** 기존 Codex credential을 복사하지 않고
  별도 browser consent, Keychain token 저장, refresh, revoke/logout, account/workspace
  mismatch와 structured inference를 live 검증한다. 공식적으로 supportable한 third-party
  integration 경계를 확립하지 못하면 unsupported로 기록하며 API-key 또는 다른 supported
  remote adapter를 유지한다.
- **S4-M3 — Sensitive routing:** domain이 purpose, data class, allowed placement,
  performance class, projection version과 external-transfer consent를 포함한
  `InferencePolicyDecision`을 model 선택 전에 만든다. Device-only raw data는 remote로
  보내지 않고 local-only route는 remote로 silent fallback하지 않는다. outbound capture
  fixture로 raw Health, Screen Time, precise location, credential과 비승인 mail body가
  process/device boundary를 넘지 않음을 검증한다.

### Connector acceptance

- **S4-C1 — Common contract:** Connections에서 provider, execution location,
  granted scopes/data types, freshness, last success/error와 disconnect/reconnect를
  확인한다. Connector는 versioned capability/View descriptor로 등록되며 Agent와
  Expert는 credential이나 provider-native object를 받지 않는다. 연결된 각 View는
  Today briefing에서 실제 소비되고, 하나가 unavailable이어도 나머지 briefing은
  source 누락을 설명하며 계속 동작한다.
- **S4-C2 — Gmail:** 실제 Google OAuth 연결에서 bounded initial import,
  `mail.search`, thread/message metadata와 on-demand body read가 동작한다. cursor 또는
  history checkpoint가 재시작 후 유지되고 revoke, expired credential, partial fetch,
  rate limit을 typed state로 복구한다. 메일 content는 untrusted data로 격리한다.
- **S4-C3 — Contacts:** 실제 Apple Contacts의 limited/full/denied 상태와 선택 변경을
  처리하고 sender/attendee를 `ExternalIdentity` evidence로 resolve한다. 연락처 전체나
  note field를 Personal Memory로 복사하지 않고 revoke 후 새 접근을 중단한다.
- **S4-C4 — Next-event feasibility:** When In Use 또는 macOS equivalent location,
  MapKit ETA와 WeatherKit current/hourly forecast로 다음 physical event의 leave-by와
  날씨 제약을 계산한다. 위치는 ephemeral/derived, ETA/날씨는 short-lived cache이며
  Always location, 이동 이력과 불필요한 route polling을 요구하지 않는다. Weather
  표시에는 provider attribution을 유지한다.
- **S4-C5 — Screen Time:** signed physical supported Apple device에서 public
  FamilyControls/DeviceActivity API의 individual authorization, entitlement와 region
  availability를 확인하고 가능한 경우 coarse `AttentionStateView` 하나를 만든다.
  private database를 읽지 않으며 raw app/domain usage나 shield mutation을 Agent에
  노출하지 않는다. 공식 경계가 signal을 제공하지 못하면 원인과 지원 불가 capability를
  ADR에 기록하는 것이 gate 결과이며 private workaround로 acceptance를 만들지 않는다.
- **S4-C6 — Apple Health:** signed physical iPhone/iPad에서 HealthKit availability와
  최소 read type 권한을 요청하고 raw sample로부터 coarse `HealthStateView`를 로컬에서
  만든다. raw sample은 Agent, Go server, log에 전달하지 않고 denial/revocation,
  no-data, stale source와 외부 변경을 구분한다.

### 구현 increment

1. fixture model의 한 turn을 typed event stream으로 assistant panel에 표시·저장한다.
2. multi-turn resume, streaming stop/retry와 session failure recovery를 연결한다.
3. fixture/local/remote model adapter와 명시적 inference policy decision을 연결한다.
4. Codex authentication feasibility와 supported remote fallback을 live 검증한다.
5. registry/assignment와 Expert 구현을 같은 host contract로 실행한다.
6. common ConnectorConnection/View fixture를 Communication/Health/Schedule Expert에 연결한다.
7. live Gmail read/search와 on-demand body를 local Go connector로 검증한다.
8. Contacts identity와 location/ETA/weather 기반 next-event feasibility를 연결한다.
9. physical Apple device에서 Screen Time gate와 Health derived-only connector를 검증한다.
10. source cohort의 일정 질문과 일정 변경 요청을 처리하고 Expert 결과를 S3
    ActionProposal에 연결한다.
11. live model, placement/consent denial, injection, budget/stall/cancel 회귀를 검증한다.

**제외:** Gmail send/archive, Contacts write/note import, Always location/history,
Screen Time restriction/shield, raw Health sync/diagnosis,
Apple source의 macOS 직접 접근이나 cross-device delivery, durable Personal Memory,
자가개선, arbitrary code/Wasm Expert, Marketplace, background execution, voice,
multi-agent persona/chat, Expert 직접 mutation, model training/fine-tuning, 중앙
semantic smart router, local-only 요청의 자동 remote fallback, 실험적 Codex OAuth의
production 지원 보장.

## S5 — Governed Context, Memory and Self-Improvement

**사용자 결과:** Floe가 이전 대화의 확정된 선호·약속을 다음 대화에서 적절히
기억한다. 사용자는 무엇을 왜 기억했는지와 Floe가 학습한 반복 절차를 검토하고,
수정·되돌리기·고정·보관·완전 삭제할 수 있다. 사용자는 Floe의 Persona를 별도로
설정하고 실제 model call에 조립된 context의 출처와 크기를 확인할 수 있다.

**의존성:** S4 Accepted, P0-D versioned corpus, P0-F local vault/key boundary.
S4 session, tool/Expert outcome과 명시적 user correction만 첫 learning evidence로 쓴다.

Personal Memory, searchable Session Archive, procedural Playbook을 별도 저장·retrieval
정책으로 다룬다. Hermes의 background review와 Skills/Curator mechanism을 참고하되,
Floe의 inferred durable write는 기본적으로 공용 Review에 staging한다. 자가개선은
externalized knowledge 개선이며 model weights, identity, safety policy, permission을
수정하지 않는다. 구체적인 ContextEnvelope, Persona/User Model 경계, retrieval manifest와
nested Playbook progressive disclosure는
[ADR 0017](../../decisions/0017-agent-context-assembly.md)을 따른다.

### Acceptance criteria

- **S5-A1:** immutable session/evidence/outcome에서 typed MemoryCandidate와
  PlaybookChangeCandidate를 만들고 source, extractor/prompt version, confidence,
  fact/inference, before/after diff와 actor를 보존한다. 재처리는 중복되지 않는다.
- **S5-A2:** background Learner는 bounded digest와 read-only evidence/evaluation만
  받고 candidate 외의 tool을 사용할 수 없다. foreground와 별도 budget/cancellation을
  가지며 새 user turn이 local review를 defer/preempt해도 session을 손상시키지 않는다.
- **S5-A3:** Behavior Kernel, Role, Persona, capability guidance, Playbook, contextual
  data, conversation과 runtime state가 typed envelope/manifest로 분리된다. Persona는
  inspect/edit/reset 및 `SOUL.md` import/export가 가능하지만 policy, Role이나 authority를
  바꿀 수 없고 Expert에는 명시적으로 필요한 경우에만 전달된다.
- **S5-A4:** candidate는 기본적으로 Review를 거쳐야 활성화된다. rejected/pending
  항목은 context에 들어가지 않는다. root Playbook 요약만 먼저 노출하고 parent를
  load한 뒤에만 direct child 요약을 발견할 수 있다. Memory와 external evidence를
  instruction으로 승격하지 않는다.
- **S5-A5:** Memory/Playbook의 source, revision, usage/outcome을 inspect/edit/delete할 수
  있다. 모든 변경은 ledger와 rollback material을 남기고 pin을 존중한다. 자동 curator는
  stale/archive만 하며 hard delete나 consolidation은 기본적으로 수행하지 않는다.
- **S5-A6:** 재시작, context compaction, extractor/Playbook version 변경 뒤에도 decision과
  tombstone이 일관되고 삭제 항목이 부활하지 않는다. replay set에서 false memory,
  retrieval precision, task outcome과 regression을 비교하며 실제 데이터는 local
  at-rest/key-unavailable gate 통과 후에만 dogfood한다.

### 구현 increment

1. session search/compaction과 recovery pointer를 구현해 S4 대화를 재개·검색한다.
2. typed `ContextEnvelope`/manifest와 component별 budget·inspection을 구현한다.
3. versioned Persona/User Model과 `SOUL.md` import/export를 분리해 연결한다.
4. correction에서 MemoryCandidate를 만들고 Review/confirmed retrieval을 연결한다.
5. nested Playbook load와 staged activation/rollback을 연결한다.
6. isolated Learner, ledger, pin/stale/archive와 삭제 전파를 구현한다.
7. corpus/replay, key failure와 실제 대화 dogfood를 검증한다.

**제외:** 무검토 inferred write, model fine-tuning/weight update, safety/policy 자가수정,
Wasm/code Playbook 자동 설치, autonomous consolidation, cross-device Memory sync.

## S6 — Transcription and Voice Mode

**사용자 결과:** 사용자가 assistant panel에서 press-to-talk로 Floe와 대화하고,
명시적으로 시작한 짧은 전사 session의 transcript·요약·Task/Commitment candidate를
시간 근거와 함께 검토한다. text, voice와 transcription이 같은 Review/Activity
경계를 사용한다.

**의존성:** S5 Accepted, streaming/recording STT, TTS와 audio-session PoC. wake
word는 요구하지 않는다. 첫 전사는 foreground microphone 또는 test audio file로
제한하고 system-wide meeting audio capture는 provider PoC 결과에 따라 결정한다.

### Acceptance criteria

- **S6-A1:** mic permission, device 선택, 시작/중지, partial/final transcript와 오류
  복구가 가능하다. audio retention/전송 상태를 녹음 전에 표시한다.
- **S6-A2:** final transcript가 S4의 동일 AgentCommand로 들어가며 text와 voice turn,
  tool/Expert call, Review action이 하나의 session 순서로 유지된다.
- **S6-A3:** TTS 중 user barge-in이 출력을 즉시 멈추고 새 turn을 시작한다. 취소된
  partial transcript나 TTS가 Memory/Learning evidence로 확정되지 않는다.
- **S6-A4:** 명시적으로 시작·종료한 TranscriptionSession이 audio source, timecoded
  final segments와 retention state를 보존한다. Summary/Task/Commitment/Memory output은
  source segment로 돌아갈 수 있는 candidate이며 자동 확정되지 않는다.
- **S6-A5:** 고정 audio corpus와 live 환경에서 first-partial/final/first-audio latency,
  WER/고유명사·날짜·금액 오류, echo/noise, CPU/battery와 민감 데이터 경계를 기록한다.

**제외:** wake word, always-on mic, verified speaker identity, background recording,
무제한 장시간 전사, cross-device handoff.

## S7 — Local Wake-up and Ambient Invocation

**사용자 결과:** macOS에서 UI가 닫혀 있어도 사용자가 명시한 wake phrase로 Floe
voice session을 열고, 눈에 보이는 listening state에서 요청을 이어간다.

**의존성:** S6 Accepted, on-device wake model과 resident Device Agent lifecycle PoC.

### Acceptance criteria

- **S7-A1:** wake detection과 제한된 pre-roll은 로컬에서 수행되며 opt-in, pause,
  명확한 listening indicator와 즉시 disable/delete를 제공한다. pre-wake audio는
  trigger 실패 시 외부 전송·영구 저장되지 않는다.
- **S7-A2:** UI 종료와 Device Agent 재시작 후에도 설정에 맞게 동작하며 중복 Agent
  session이나 동시에 두 microphone owner를 만들지 않는다.
- **S7-A3:** wake 후 S6 session으로 handoff하고 timeout/cancel/lock-screen 상태를
  안전하게 처리한다. speaker match는 편의 신호일 뿐 민감 작업의 승인 권한이 아니다.
- **S7-A4:** 다양한 거리·소음·유사 발화에서 false accept/reject, wake latency,
  CPU/battery를 측정하고 정한 threshold를 통과하지 못하면 hotkey/수동 voice로
  fallback한다.

**제외:** iOS always-on wake, 회의 상시 녹음, cross-device arbitration, proactive
intervention, biometric authorization 대체.

## S8 — Same Loop Across Devices and Server

**사용자 결과:** 한 기기에서 확정한 Memory와 수행한 제안·실행 결과를 다른
기기에서 확인한다.

**의존성:** S7 Accepted, sync·identity·storage 보안 PoC.

최소 Go 서버와 재현 가능한 self-host 실행 경로, 두 클라이언트만 다룬다.
실제 기기/플랫폼 조합, provider 실행 위치, sync topology는 착수 전에 결정한다.
두 로컬 인스턴스 데모만으로 cross-device 검증을 완료하지 않는다.

### Acceptance criteria

- **S8-A1:** clean environment에서 문서화된 절차로 최소 서버를 실행하고 실제
  두 기기를 같은 Person에 인증·연결할 수 있다.
- **S8-A2:** 일정, Agent session lineage, confirmed Memory/Playbook revision/tombstone,
  active Expert assignment와 Review/실행 결과가 두 기기에 수렴한다. offline
  재연결, 충돌, 중복 전달로 인한
  Memory 부활이나 중복 실행이 없음을 검증한다.
- **S8-A3:** 다른 Person의 접근과 철회된 기기의 신규 접근을 차단한다.
  전송/저장 보호, credential 보관, 삭제 전파 정책을 명시하고 검증한다.
- **S8-A4:** device-native Calendar의 실행 기기가 offline이면 실행을 보류하거나
  명시적으로 실패시킨다. 서버가 로컬 OS capability를 가진 것으로 가정하지 않는다.

**제외:** 전 플랫폼 parity, multi-user 관리 UI, OAuth broker, hosted 운영 완성.

## S9 — Event-driven Intervention

**사용자 결과:** 일정 변경으로 제안이 무효해지면 적절한 시점에 재제안을 받는다.

**의존성:** S8 Accepted; S7의 local lifecycle을 재사용한다.

### Acceptance criteria

- **S9-A1:** macOS UI가 닫힌 동안에도 resident Device Agent가 일정 변경을
  감지하고 재제안한다. 프로세스 재시작 후에도 동작한다.
- **S9-A2:** quiet hours, opt-out, 중복 억제와 두 기기 사이 알림 중재를 검증한다.
- **S9-A3:** 알림에서 앱의 근거·승인 화면으로 이어지며 외부 쓰기는 S3의
  동일한 승인·검증 경계를 통과한다.
- **S9-A4:** 구조화된 dogfood 기록으로 유용한 개입, 불필요한 개입, 누락을
  평가한다. 일정/알림은 deterministic scheduler가 담당한다.

**제외:** 상시 녹음, advanced speaker recognition, 음성 handoff.

## 구현 운영과 완료 상태

1. 착수 시 slice의 선행 조건, 실제 provider, 제외 범위, 인수 조건을 확정한다.
2. 작업을 계층 전체가 아니라 앱에서 확인 가능한 작은 end-to-end 변경으로 나눈다.
3. 같은 운영 계약의 fixture connector/model로 경계를 연결한 뒤 실제 의존성을
   하나씩 교체한다. 실제 provider PoC는 초기에 실행해 후반 통합 실패를 줄인다.
4. 각 변경은 관련 테스트와 진행 기록을 포함한 논리적 commit으로 남긴다.
5. 데모와 실패 사례를 검증한 뒤 dogfood를 거쳐 인수한다.

| 상태 | 진입 조건 |
| --- | --- |
| Planned | 범위와 인수 조건이 정의됨; 구현 완료를 뜻하지 않음 |
| Implementing | 선행 조건을 확인하고 구현 착수 |
| Integrated | 앱에서 전체 경로가 동작; fixture만 쓰면 그 사실을 표시 |
| Verified | 모든 인수 조건 통과, 실제 의존성 검증과 회귀 테스트 근거 기록 |
| Dogfooding | Verified build를 실사용하며 정해진 시나리오와 문제 기록 |
| Accepted | dogfood 결과 검토, 필수 조건 충족, 차단 결함 없음 |

진행 중인 구현 slice는 하나만 둔다. 다음 slice 착수 시 기존 로컬 slice의
비차단 작업은 Deferred로 명시하고 기존 완료 기록은 유지한다. Dogfooding은
다음 slice와 병행할 수 있다. blocker는 상태와 별도로 원인·해소 조건을 기록한다.
인수 조건 회귀가 발견되면 상태를 되돌리고 근거를 남긴다.

각 slice의 dogfood 기간과 관찰 질문은 착수 시 정하고 결과를 기록한다.
이는 Personal Day MVP의 별도 2주 dogfood 요건을 대체하지 않는다.

## 검증 근거 형식

각 Sx-Ay에 다음 정보를 연결한다. 통과 수는 구현률이 아니라 검증된 조건 수다.

```text
Criterion: S1-A1
Result: pending | pass | fail
Integration: fixture | sandbox | live (connector/model/server/device별 기록)
Evidence: test command + result 또는 수동 데모 절차 + 관찰 결과
Build: 검증한 commit SHA
Date / environment: 날짜, OS, provider, 모델 및 설정(해당 시)
Known limitations / blocker: 남은 제약과 해소 조건
```

- CI/자동 회귀: 고정 clock/timezone, fixture provider, deterministic model 대역.
- 경계 테스트: connector 정규화, Rust typed command, snapshot, Flutter UI,
  proposal validation, 실행 상태 머신과 재시작 복구.
- 실제 연동 smoke test: 권한, 수집, 생성·재수집을 전용 테스트 캘린더에서 검증.
  자격증명 없는 CI에 live 테스트 성공을 요구하거나 실패를 숨기지 않는다.
- 실제 모델 평가: 동일한 시나리오 세트로 근거·시간 유효성·실패 처리를 평가하며
  문장 완전 일치와 단일 성공 응답을 품질 기준으로 삼지 않는다.
- 재현 로그: source → context → proposal → execution → projection의 ID를
  연결하되 민감 원문과 자격증명은 기본적으로 남기지 않는다.

## Phase coverage — 완료 선언이 아닌 검증 지도

| Phase | 먼저 검증하는 slice 경계 | 여전히 별도 검증이 필요한 범위 |
| --- | --- | --- |
| 0 — PoCs | S1 connector, S3 executor, S4 Agent/Expert/source contracts, S5 memory/learning, S6 voice, S7 wake, S8 sync/security | provider별 미검증 technical risk와 나머지 PoC |
| 1 — Personal Day | S1 Day Canvas, S3 승인 UI, S4 chat, S6 voice capture | 편집·folding·MVP dogfood |
| 2 — Connected | S1/S3 Calendar, S4 Gmail/Contacts/location/ETA/weather/Health/Screen Time gate와 Experts | provider parity, work/files, 추가 Expert |
| 3 — Memory | S5 대화 기반 Memory/Playbook lifecycle | People/Relationship/Episode, 외부 source, identity resolution |
| 3.5 — Experts | S4 local contract, assignment, permission, built-in/declarative 실행 | Wasm, SDK, marketplace, server placement |
| 4 — Cross-device | S8 두 기기 sync, S7 local resident lifecycle | iOS/Android/Windows 전체 경험과 voice handoff |
| 5 — Ambient | S6 voice, S7 wake-up, S9 변경 감지와 개입 | meeting transcription, advanced speaker recognition |
| 6 — Hosted/Self-host | S8 최소 Go 서버와 배포 | admin, 다중 사용자 운영, broker, hosted 완성 |

이 표는 계획된 coverage다. 실제 검증 여부는 `PROGRESS.md`에서만 갱신한다.
