# 04 — Engine·Task·Run의 checkpoint·실패·정산

대응: 계획 §3.4 / P02·P03·P04·P09·P11·P12. 다음은 [05](05-adapters.md).

## 04.1 모델 batch와 실행 cursor를 먼저 저장

**읽기:** [S03 ports:L1–80](../SOURCE_ANCHORS.md#s03), [S23 Engine:L90–245](../SOURCE_ANCHORS.md#s23), [S24 Engine:L245–475](../SOURCE_ANCHORS.md#s24), [S28 repository](../SOURCE_ANCHORS.md#s28). Engine의 `stable_invocation_key`와 Conversation `application/recovery.rs`를 같은 파일에서 찾아 읽는다.

현재 `JournalEvent::Checkpoint { iteration }`만으로 pending batch의 중간을 복구할 수 없다. 다음 정보를 같은 canonical journal 계약에 넣는다.

```text
EngineCheckpoint:
  execution_id (Run 또는 Task), executor_generation
  iteration, batch_id, validated_steps, next_step_index
  stable call/task/invocation refs, result refs
  admitted catalog definition revisions
  coverage refs, usage watermark
```

1. `ModelIntent/ModelResult`는 Inference attempt 참조를 담는다. 모델이 낸 steps 전체의 shape·byte·허용 도구/카드 revision을 검증한 뒤, **실제 tool/delegate 이전**에 validated batch를 durable journal에 기록하고 ack를 기다린다.
2. step identity는 durable execution ID + batch ID + step ordinal에서 만든다. transport `trace.request_id`, 실제 실행된 tool 개수, 재시도 때 달라지는 random batch에서 복구 identity를 다시 만들지 않는다.
3. 각 step은 `intent ack → dispatch → result ack → next index ack` 순서다. result ack 유실 시 같은 invocation/Task를 query하거나 repository가 반환한 replay receipt를 사용한다. 이미 완료된 외부 write를 새 ID로 재호출하지 않는다.
4. 저장된 batch가 있으면 모델을 다시 호출해 다른 batch를 얻어 이전 결과로 간주하지 않는다. response를 받기 전 crash의 model 비용/응답 불명은 Attempt 기록으로 구별한다.
5. `DelegationIntent`, Tool/Task result, Output, Checkpoint의 최대 크기와 전체 entry 수를 유지한다. source payload를 무제한 journal에 복사하지 않는다. engine memory map과 durable owner journal을 구별한다.
6. Conversation recovery projection과 실제 Vault journal decoder를 같은 계약으로 바꾼다. old journal을 받아 주는 version mapper는 만들지 않는다. current-schema restart/replay는 계속 지원한다.

**A 최소 안전 검사:** ack 거절 시 실제 side effect가 0인 기존 regression을 현재 계약으로 이식한다. journal을 메모리 no-op로 바꿔 통과시키지 않는다.

## 04.2 child 실패와 root fault를 분리

**읽기:** S24의 tool match 및 delegation `.await?`, [S26 TaskRecord/Repository:L1–200](../SOURCE_ANCHORS.md#s26), TaskCoordinator의 `execute/cancel_task`, Conversation coordinator의 Engine result match.

1. `ScopedIssue`에는 reason·origin·scope kind/id·recovery class·safe message code를 둔다. provider raw exception·credential·source 제목·stacktrace를 LLM observation으로 전달하지 않는다.
2. Task owner가 `Rejected/Failed/TimedOut/Cancelled/Interrupted`를 durable 결과로 반환하게 한다. root는 해당 TaskReceipt를 관측한다. endpoint가 실패했다고 root에 `cancel()`을 전파하지 않는다.
3. known tool denied/unavailable, child deadline과 root의 명시 cancel/hard deadline, Vault/identity/journal fault를 구별한다. 동일 `DeadlineExceeded` enum만 보고 모두 같은 처리로 보내지 않는다. root scope가 아직 유효한지 확인한다.
4. 실패 Task의 stored coverage가 Unknown이면 payload를 공개하지 않는다. host가 만든 **payload 없는 상태 안내**만 독립적인 observation으로 새로 만든다. source 내용이 있는 Unknown 데이터를 Independent로 재라벨링하는 것이 아니다.
5. 매번 실패한 동일 capability를 무한 재시도하지 않게 기존 제한을 유지한다. root는 다른 수단 선택 또는 제한 설명으로 끝낼 수 있다. 실패를 Completed Task로 기록하지 않는다.
6. Engine가 `Err`만 반환해 부분 결과를 잃는 경우 `EngineExit { report, stop }`와 동등한 하나의 반환 계약으로 수정한다. 이미 저장된 steps/attempts/coverage를 finalization이 회수할 수 있게 한다. 새 parallel Engine 구현을 만들지 않는다.

**삭제:** child transport 오류의 무차별 root `?`, 오류 code 하나로 UI 전체 Vault reset, 실패를 성공으로 위장하는 경로.

## 04.3 finalization과 업무 결과를 별도로 저장

**읽기:** `crates/modules/conversation/src/application/finalization.rs`, coordinator의 `finalize_exhaustion`, [S08 저장 state validation](../SOURCE_ANCHORS.md#s08), [S32 wire 변환](../SOURCE_ANCHORS.md#s32).

1. root report는 `execution=Completed/Partial/Blocked/Failed/Cancelled/Indeterminate`와 `reply=Generated/PolicyNotice/NotProduced`를 분리한다. lifecycle Finished는 task 성공을 뜻하지 않는다.
2. finalization은 **tool/delegation 없는** 호출이다. 이미 승인된 현재 projection·확정된 observations만 입력으로 쓴다. 부분 Task failure뿐 아니라 budget/stalled도 안전한 조건에서 설명한다.
3. work budget과 finalization reserve를 root 총량 안에서 나눈다. 정책 기본은 최대 한 번, 출력 최대 1024 tokens, 남은 root hard deadline 안에서 최대 10초다. 실제 SDK의 입력 token 사용량과 최종 비용도 root 총량에 포함한다. 이 값은 설정 정책이지 측정된 최적값이 아니다.
4. work child deadline은 finalization 여유를 남길 수 있게 root보다 짧게 정한다. root cancel/hard deadline/Vault unavailable/무승인 recipient에는 새 scope를 만들어 한도를 연장하거나 추가 모델을 호출하지 않는다.
5. 저장에서 `Failed + output`을 budget/stalled 두 오류에만 제한하던 predicate를 report 기반으로 교정한다. 허용된 결과 공개와 실패 사실이 동시에 남아야 한다. `output.is_some()`을 Completed 판정으로 사용하지 않는다.
6. wire DTO·FloeClient decoder·UI error presentation을 동일 report로 바꾼다. deterministic policy notice도 LLM 생성문처럼 위장하지 않는다.

## 04.4 Inference에 단일 attempt accounting을 모음

**읽기:** [S23 Engine reserve/mark_dispatched/settle](../SOURCE_ANCHORS.md#s23), [S25 AttemptLifecycle:L1–85](../SOURCE_ANCHORS.md#s25), Inference model transport의 직접 소비자와 Knowledge learner model adapter.

1. 현재 `AttemptLifecycle::start`의 내부 random ID 발급을 호출자가 전달하는 stable `attempt_id`로 바꾼다. 이 ID는 ModelPort 결과·Inference store·Engine journal에서 동일해야 한다.
2. Engine는 시도 상한과 scope를 ModelPort에 전달하고 AttemptReceipt를 기록한다. budget reserve·실제 handoff marker·usage settle은 **Inference service 한 곳**에서 수행한다. Engine의 중복 `.begin/.mark_dispatched/.settle`를 삭제한다.
3. queued/reserved, intent-acknowledged, handed-off, completed/unknown usage를 구별한다. worker queue에서 취소된 요청을 발송된 것으로 계상하지 않는다. durable intent는 발송 사실과 같지 않다.
4. provider response가 usage를 주면 같은 attempt에 한 번 settle한다. ack 유실·socket 오류로 실제 사용량을 모르면 bounded unknown estimate를 저장한다. 추후 확정 receipt는 차이만 반영하며 두 번 더하지 않는다.
5. Attempt store의 CAS는 stale writer/generation, 이미 settled ID의 다른 결과를 거절한다. 현재 `AttemptJournal`를 필요한 원자 port로 확장한다. provider adapter에 별도의 과금 원장을 만들지 않는다.
6. Learner와 Expert도 같은 ModelPort/Inference 흐름을 사용한다. 각 자식의 budget lease는 부모 총량에서 할당하며 루트 전체 예산을 복제하지 않는다.

## 04.5 취소·terminal·공개 fence

1. TaskCoordinator의 active map은 TaskId→child cancellation/join만 보관한다. durable Task state는 repository가 원본이다. child token을 부모 clone과 혼동하지 않는다.
2. Run/Task가 cancel 또는 terminal된 뒤의 late endpoint/model callback은 executor generation과 현재 authority로 거절한다. 새로운 실행에 예전 output을 붙이지 않는다.
3. Context의 source-derived output/summary/replay는 input provenance를 전이적으로 유지한다. source 없이 계속하려면 원문·파생 결과·pending output·provider 대화 context까지 안전하게 재구성한다.
4. Access의 acquire/dispatch/release 검증은 실제 경계 직전에 수행한다. 철회는 durable authority 갱신과 짧은 release fence 뒤 ack하며 event listener의 성공에 의존하지 않는다. fence 안에서 전체 네트워크 응답을 기다리지 않는다.
5. UI에 공개된 내용을 나중에 회수할 수 있다고 가정하지 않는다. source-dependent text는 release 전까지 버퍼링하고 개인정보 없는 진행 상태만 먼저 관측시킨다.

**단계 A 완료:** Engine·Task·Conversation·Inference·저장·wire의 의미가 일치한다. mock-only journal, 이름만 추가한 outcome, 중복 usage owner가 없다. 변경한 intent/권한/취소 의미만 좁게 확인했다. live LLM 품질과 전체 fault matrix는 B에 남긴다.
