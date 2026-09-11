# Connection Authorization Runtime

> Status: Incremental implementation; native Calendar authority and lazy access validation implemented — 2026-09-12
>
> 아래 전체 runtime은 목표 설계다. 실제 구현 범위와 남은 경계는 §0을 따른다.

공통 의미와 사용자 동의 규칙은
[Connection, Access & Observation](../05-integrations/connection-access-and-observation.md),
설계 결정은 [ADR 0027](../../decisions/0027-connection-authority-and-observation.md)을 따른다.

## 0. 첫 구현 범위

- `floe-domain::SourceAuthority`는 random incarnation과 checked positive epoch를 가진 공통 primitive다.
  Calendar mirror의 `revision`은 기존 CAS/write counter로 유지한다. 정상 sync·일시 장애·표시명 변경은
  authority를 유지하고, scope/resource identity 변경·disconnect·새 permission denial은 이를 바꾼다.
- Calendar connection의 `source_authority`는 필수다. native setup/receipt/view도 유효한 stamp가 없으면
  registry restore/설치를 거부한다. grant 필드의 optional 형태는 별도 server/fixture 계약을 위한 것이며
  native legacy 허용이 아니다. FFI는 사용자 setup/scope
  요청의 exact Person/connection/device/provider/revision과 허용 subset을 확인한 뒤 Core stamp를 기록한다.
  caller가 보낸 stamp는 신뢰하지 않는다. setup retry는 저장된 stamp를 유지한다.
- 하위호환 migration/fallback은 지원하지 않는다. 누락된 source stamp를 sync에서 새로 채우던 경로는
  제거했다. 지원하지 않는 저장 형식은 실패로 반환하며 사용자 DB/vault를 삭제하거나 재생성하지 않는다.
  유효한 현재 grant의 실제 authority 변경은 `access_review_required`로 차단한다. 연결 설정 변경이
  AI grant를 자동 생성·갱신·확장하던 UI 경로도 제거되어 있다.
- native observation publication은 이제 `connection_id`를 필수로 보낸다. FFI가 현재 source와 device,
  provider, CAS revision, 전체 resource ID를 대조해 stamp를 붙인다. Flutter와 native bridge는 함께
  배포해야 하며 구 wire 요청을 권한 검증 없이 fallback하지 않는다.
- connection observation은 최대 128개 source와 총 10,000개 record를 받되 AI grant의 4개 제한은 유지한다.
  read adapter는 허용된 subset만 projection하고, 같은 turn에서는 첫 observation을 pin한다. 매 check에서
  현재 source authority·철회·permission failure·observation 유효성을 다시 확인하며 부분 실패를 빈 성공으로 바꾸지 않는다.
- 이 단계는 기존 encrypted registry를 grant 저장소로 사용한다. 별도 `DataAccessGrant` store, policy epochs,
  headless native acquisition/refresh, cross-device owner verification, durable lineage cleanup,
  action lifecycle 통합은 아직 구현하지 않았다. OS revoke는
  native sync 결과가 Core에 반영된 이후 검증하며 즉시 OS notification/fence를 구현했다고 주장하지 않는다.
- Google/Microsoft와 다른 server connector는 기존 계약을 유지한다. local mirror counter와 server revision의
  의미를 합치거나 서버 grant 검증 완료로 간주하지 않는다. 아래 A–F acceptance를 완료 처리하지 않는다.

첫 increment 검증: domain/agent/core/protocol/ffi Rust suite, native bridge build와 Calendar publication
C ABI identity test를 실행한다. Flutter Calendar access/observation/recovery tests와 analyze도 검증한다.
확장 agent UI suite의 registry dialog 3건과 proposal card golden 1건 실패는 변경 전 HEAD에서도
동일하게 재현되어 이 작업에서 수정하지 않는다. Dart/C ABI fixture의 기본 30초 timeout은 별도 실행의
2분 제한에서 통과했다. 실행 중인 사용자 앱이나 실제 OS 계정의 권한은 테스트에서 변경하지 않는다.

### 후속 increment: 대화 시작과 source 사용 분리

- Flutter의 `beforeInvocation` Calendar refresh hook을 제거했다. 주기적/resume refresh는 producer 역할로
  남지만 일반 메시지 전송의 성공 조건이 아니다. 아직 tool 호출이 OS refresh를 직접 시작하지는 않는다.
- FFI는 활성 grant metadata로 실행 경로만 준비한다. 현재 connection 조회, authority/revision/identity
  비교, scope 검증은 `BoundAccess`의 실제 Calendar View 접근 시 수행한다. device 후보가 없거나 여러 개면
  임의 source로 우회하지 않고 해당 호출에만 review failure를 반환한다.
- source를 사용하지 않은 turn은 Calendar 권한이 없어도 답변을 완료할 수 있다. 실제 읽기가 실패하면
  typed delegation failure를 Manager에 전달한다. 읽은 이후에는 기존 source/registry 재검증 및 commit
  hook을 유지하므로 철회된 결과를 성공 답변으로 commit하지 않는다.
- durable dependency lineage 이전의 보수적 model projection을 추가했다. 이전 Calendar 결과 이후의
  생성 메시지 및 출처 정보 없는 compaction 요약은 다음 turn 모델 입력에서 제외하고 provider replay도
  비운다. 사용자 입력과 source 사용 전 일반 대화는 유지하며 저장된 원문을 삭제하지 않는다. 따라서 과거
  일정에 대한 후속 질문은 재조회가 필요하다. 이 projection은 활성 Calendar grant가 없는 일반 경로에도 적용된다.
- Calendar evidence 또는 출처 불명 요약을 포함한 soft-stop continuation은 새 lease 없이 재개하지 않는다.
  새로운 요청에서 다시 읽어야 한다. 이는 durable resume lease/정밀 lineage 구현 완료가 아니다.
- Calendar setup이 먼저 존재할 때 builtin 초기화가 default-off 저장 규칙과 충돌하던 경로를 수정했다.
  scoped `install_builtin_experts_enabled`만 해당 신규 receipt의 enabled 설치를 허용한다. 일반 registry
  저장으로 enabled receipt를 주입하는 동작은 계속 거부하며 기존 Calendar grant는 변경하지 않는다.

후속 검증에는 권한 거부 상태의 일반 답변과 Calendar 호출 실패를 실제 FFI worker 및 loopback model
server로 확인하는 테스트, 이전 결과/요약 projection, 엄격한 저장 계약 및 scoped install tests를 포함한다.
`cargo test --workspace`, `cargo fmt --all -- --check`, `cargo build -p floe-ffi`가 통과했다.
Flutter analyze, 관련 UI/observation 42개 테스트와 재빌드한 bridge의 Dart/C ABI 8개 테스트도 통과했다.
native 테스트 서버의 accepted socket은 명시적으로 blocking mode로 전환하며 FFI suite를 반복 검증했다.

## 1. 최초 분석 시점의 코드와 설계 간 차이

| 현재 경로 | 관찰한 동작 | 바꿀 경계 |
| --- | --- | --- |
| `crates/floe-core/src/calendar.rs` | `import_calendar_sources` 등에서 sync 성공/실패도 connection revision 증가 | source authority와 mirror write version 분리 |
| `crates/floe-agent/src/registry/builtin_setup/schedule.rs` | durable setup/view에 connection revision 저장 | `DataAccessGrant`를 authority로, registry는 제한된 execution projection으로 |
| `crates/floe-ffi/src/vault_host/conversation_turn/expert_dispatch/schedule.rs` | 현재 connection과 setup/view의 revision·resource list를 exact 비교 | source stamp 검증 후 grant subset projection |
| `apps/client/lib/features/day_canvas/presentation/personal_day_screen.dart` | 모든 turn의 `beforeInvocation`에서 calendar sync; 설정 reconciliation은 별도 UI 경로 | headless-capable tool-time acquisition |
| `apps/client/lib/features/day_canvas/application/calendar_observation_publisher.dart` | 연결 캘린더 4개 초과 시 observation revoke | 전체 mirror와 bounded AI View를 분리, budget failure를 permission과 구별 |
| `crates/floe-ffi/src/local_context.rs` | calendar observation을 Person/device당 최신 한 개로 대체하고 connection revision exact 비교 | 정확한 connection과 immutable observation handle 기반 저장 |
| `server/internal/console/store.go`, `client_connectors.go` | Person-owned connection record의 revision으로 scope mutation/disconnect를 보호 | server source epoch와 local mirror counter를 다른 type으로 |
| `server/internal/console/console.go` | Calendar View는 connection ID/revision을 요구; Mail 등은 owned runtime 선택 방식이 다름 | 모든 protected View의 exact connection/grant 검증 port |
| `crates/floe-agent/src/connected_context.rs`, `server/internal/connectors/common/contract.go` | lifecycle/descriptor/freshness/provenance 공통 계약 존재 | 계약을 확장하되 새로운 병렬 connector framework는 만들지 않음 |
| `apps/client/lib/features/agent/agent_controller.dart`, `agent_panel.dart` | `stale_context`를 Calendar access 메시지로 표시하고 실패를 reload 상태로 처리 | source-local recovery와 session recovery 분리 |

이는 코드 경로 분석이다. 모든 connector에 동일 버그가 재현됐다는 뜻도, 사용자의 encrypted
grant를 검사해서 정확한 이전 revision을 확인했다는 뜻도 아니다. 특히 server connection
revision은 local Calendar mirror revision과 이미 다른 의미이므로 숫자를 단순 통합하지 않는다.

## 2. 책임과 authority owner

| Component | 책임 | 하지 않는 일 |
| --- | --- | --- |
| Flutter | consent/recovery UI, user intent, typed status 표시 | grant 자동 생성·확장, epoch 재작성, permission 판단 |
| Rust Core + encrypted vault | local grant owner, authorization, snapshot acquisition, turn dependencies, action policy | 원격 provider secret 관리 |
| Native Swift/Kotlin adapter | 실제 OS permission/identity 확인, bounded read와 privacy projection | user consent 발급, 다른 기기 권한 대행 |
| Go connector host | server connection authority, provider credential·scope, 요청 admission과 producer-side 검증 | paired credential만으로 AI grant 또는 external transfer 동의 추정 |
| Manager/Expert | bounded View로 판단, typed tool 요청과 proposal | raw connector/store/network access, lease 발급 |

각 connection에는 한 source authority owner, 각 grant에는 한 grant authority owner가 있다.
초기 local grant owner는 encrypted vault를 가진 Core다. 서버에 connector가 있다는 이유만으로
grant owner가 서버로 바뀌지 않는다. 잠긴 vault에서 authority를 읽을 수 없으면 lease를 발급하지
않는다. S8의 owner 이전은 인증된 handover와 새 incarnation 없이는 허용하지 않는다.

source epoch는 execution owner가 발급한다. client가 받은 remote epoch를 local sync 성공 시
증가시키지 않는다. grant와 정책 변경은 각각의 owner가 직렬화한다. raw source authority와
grant authority 사이에 전역 transaction이 있다고 가정하지 않고 두 stamp를 모두 검증한다.

remote producer는 pairing과 별도로 해당 grant owner의 검증 가능한, audience-bound 사용 허가를
요구한다. 첫 remote 구현은 producer가 신뢰하는 authority record/online verification으로
검증하고, caller가 body에 적은 `access_epoch`나 `allow=true`를 신뢰하지 않는다. 단순 bearer
pairing을 grant로 승격하지 않는다. owner 인증·revocation freshness가 없는 remote path는
일반 대화만 허용하고 protected source를 typed unavailable로 반환한다. cross-device relay와
offline permit transport는 ADR 0024/S8 gate이며 이 문서가 그 구현 완료를 의미하지 않는다.

## 3. 최소 내부 계약

다음은 semantic sketch다. 도메인 type은 `floe-domain`, wire DTO는 `floe-protocol`,
검증/저장 orchestration은 `floe-core`, transport/host adapter는 `floe-ffi`와 Go에 둔다.
Rust agent runtime type을 protocol dependency로 되돌리지 않는다. 공통 Rust/Go fixture로
동일한 규칙을 검증하고, 이 설계만을 위해 새 서비스나 crate를 먼저 만들지 않는다.

```text
ConnectionKey = (person_id, connection_id)
SourceStamp = (ConnectionKey, execution_owner, source_epoch)
GrantStamp = (person_id, grant_id, authority_owner, access_epoch)
PolicyStamp = (policy_id, authority_owner, policy_epoch)

AcquireView {
  invocation_id, grant_id, connection_id, capability_id
  requested_scope, purpose, consumer
  query, max_age, deadline, item_limit, byte_limit
}

AuthorizedViewLease {
  lease_id, invocation_id, source_stamp, grant_stamp, policy_stamps[]
  consumer, audience, purpose, effective_scope_handle
  observation_id, issued_at, expires_at
}

ContextDependency {
  source_stamp, grant_stamp, policy_stamps[]
  observation_id, effective_scope_handle, observed_at, expires_at
}
```

`person_id`와 caller identity는 local authenticated host context 또는 paired identity에서
유도한다. 요청의 `consumer`/purpose/scope는 원하는 상한이지 권한 증명이 아니다. host는
현재 installation/assignment와 허용 purpose에 대조한다. lease는 in-process에서는 opaque
handle이고 외부 protocol에서는 issuer/audience/replay 검증이 필요하다. LLM에게 lease
secret, grant mutation API, raw scope mapping이나 credential을 주지 않는다.

`policy_stamps`에는 processing 정책뿐 아니라 실제 consumer의 활성화/assignment/manifest 권한
정책도 포함한다. Expert 비활성화나 permission 변경이 진행 중 lease에도 반영돼야 한다. 관련 없는
Expert 설치로 모든 작업을 무효화하는 global registry revision은 이 stamp를 대신하지 않는다.

`source_epoch`, `access_epoch`, `row_version`은 서로 대입할 수 없는 type으로 감싼다.
storage CAS version은 repository 내부에 남기고 consent command는 변경 대상의 access epoch와
사용자가 확인한 source/policy stamp를 기대값으로 사용한다. health update가 consent dialog를
불필요하게 충돌시키지 않되, 실제 scope/recipient가 바뀌면 stale review를 거부한다.

Grant/registry execution projection은 authority store 안에서 원자적으로 변경하거나
projection epoch mismatch 동안 deny한다. 두 저장소에 best-effort dual write 후 성공이라고
응답하지 않는다. display name 같은 metadata 갱신은 해당 row version만 바꾼다.

## 4. Tool-time acquisition

1. Manager가 source data 없는 capability metadata로 필요한 도구를 선택한다.
2. Core가 현재 source/grant/processing stamps와 effective scope를 확인한다.
3. 정확한 connection·scope·query를 만족하는 unexpired observation을 찾거나 owner에 bounded
   refresh를 요청한다. 모든 connector를 먼저 동기화하지 않는다.
4. producer는 read 시작 시 authority를 확인하고, 반환 직전에도 현재 source authority를
   검증한다. 검증 사이에 권한이 바뀌면 그 결과를 발행하지 않는다.
5. Core는 grant/consumer/transfer 상태를 다시 확인하고 허용 subset의 View만 투영한다.
6. source/grant/policy가 바뀌지 않았음을 fence로 확인한 뒤 immutable lease를 발급한다.
7. Agent가 View를 사용한 순간 dependency를 기록한다. 새 observation은 기존 handle을 덮어쓰지
   않는다. 다른 도구는 별도 lease를 쓸 수 있고 evidence의 시점 차이는 보존한다.

cache key는 최소 `(person, connection, producer, source_epoch, view/version, scope, query)`를
구분한다. grant-filtered projection cache에는 grant/access epoch와 processing policy도 포함한다.
query에는 Calendar 시간대·범위, pagination, Mail body ID, ETA origin/destination 같은 domain
의미가 포함된다. provider-native ID는 private mapping에만 두며 model-visible handle로 대체한다.

한 턴에 global atomic snapshot을 강제하지 않는다. 여러 source를 쓰면 dependency set을 고정하고
domain이 허용하는 시점 차이/coverage를 검증한다. provider가 stable pagination snapshot을
지원하지 않으면 bounded materialization 또는 partial/disagreement를 반환한다. 서로 다른
페이지를 하나의 완전한 시점이라고 주장하지 않는다.

single-flight refresh는 같은 authorization/query에만 공유한다. 취소·scope 변경한 요청과 결과를
섞지 않는다. 재획득은 invocation deadline 안에서 제한된 횟수만 허용하고 무한 retry하지 않는다.
초기 구현은 invalidated lease당 자동 재획득 최대 1회이며, side effect의 자동 replay는 없다.

## 5. 실행 중 변경과 output fence

```text
tool read → bounded observation → grant projection → model input
                                                 → expert artifact
                                                 → manager input
                                                 → final output / proposal
```

각 화살표는 개인정보를 사용할 수 있는 경계다. 다음 model request나 egress 전에 dependency와
transfer policy를 검증한다. 소비한 dependency의 합집합을 model output에 보수적으로 상속한다.
model이 “이 source를 사용하지 않았다”고 주장해 dependency를 지울 수 없다.

- background sync: pinned observation이 아직 유효하면 실행을 계속한다.
- source/grant/policy epoch 변경: 해당 lease를 무효화하고 관련 in-flight context를 취소한다.
- expiry: 오래된 evidence의 최신 사실 주장이나 새 사용을 중단한다. fresh evidence를 얻어
  재판단하거나 현재성 미확인으로 응답한다. 과거 source-backed 주장으로 보존하려면 별도 history
  정책이 필요하며 lease expiry를 연장하지 않는다.
- 여러 source 중 하나가 철회됨: 그 source를 본 모델의 미공개 응답 전체를 격리한다. 다른
  source만으로 계속하려면 깨끗한 context로 새 inference를 수행한다.

authority mutation, lease admission, output release decision은 owner의 짧은 critical section 또는
동등한 serialized fence에서 순서를 정한다. model/provider 작업 내내 lock을 잡지 않는다.
final output, artifact commit, proposal publish에도 검증한다. streaming은 각 chunk release가
fence를 통과하거나 최종 검증 전까지 source-derived text를 buffer한다. 초기는 buffering으로
시작하고 release 직전 검증을 생략한 token streaming을 허용하지 않는다.

철회가 fence보다 먼저 commit되면 새 release를 거부한다. fence를 통과해 transport에 넘긴 bytes,
이미 model/provider에 전달한 데이터, 이미 표시된 chunk는 회수할 수 없다. cancellation은
best effort이며 completion을 허용한다는 뜻이 아니다. timeout 뒤 도착한 callback도 기존
invocation/stamps에 묶어서 discard한다.

dependency는 conversation context, Expert private state, cached summary, continuation,
proposal 및 Learner candidate에 전파한다. reload/resume/retry는 현재 권한으로 재검증하며
archive 전체를 무조건 prompt에 재삽입하지 않는다. provenance가 없는 legacy source-derived
history는 새 model context에서 격리한다. 이미 사용자에게 전달한 archive와 앞으로 AI가
재사용할 수 있는 working context는 별도 projection이다.

## 6. Observe와 Act의 분리

Act는 source/grant 검증 외에 별도의 Action authority와 exact-target approval이 필요하다.
승인 record에는 Person/connection/target/action type/정규화 payload digest/허용 실행 조건/
expiry를 묶는다. local Model의 판단이나 valid Observe lease는 이 승인을 대체하지 않는다.

실행 직전 executor는 다음을 검사한다.

1. 현재 Person, account, execution owner, resource identity와 source 권한.
2. 현재 Act 정책, approval 내용·expiry, 취소/철회 여부.
3. 변경할 item의 provider version/ETag와 작업별 precondition.
4. create/move 일정의 availability를 주장했다면 해당 시간대의 재확인.
5. 동일 logical action의 durable idempotency/attempt 상태.

관련 없는 새 메일이나 다른 날짜 일정이 바뀌었다고 전체 approval을 취소하지 않는다. 반대로
target/payload가 바뀌면 기존 approval을 수정해 재사용하지 않는다. 검증 실패는 새 제안/승인으로
복구한다. provider가 조건부 쓰기를 지원하면 그 조건을 요청에 포함한다. 지원하지 않으면
검증과 실제 실행 사이 경쟁이 남음을 기록하고, 위험상 허용할 수 없는 action은 unsupported다.

```text
prepared → approved → dispatch_admitted → succeeded | failed | unknown
```

dispatch admission과 revoke/cancel은 executor에서 직렬화한다. admission 전에 철회되면 실행하지
않고, provider에 전달된 뒤에는 취소·성공 여부를 별도로 확인한다. provider가 idempotency를
지원하지 않으면 local key만으로 exactly-once라고 주장하지 않는다. crash/timeout 후 결과가
`unknown`이면 durable attempt를 조회·reconcile하고, 확인 없이 재실행하거나 새 key로 바꾸지 않는다.

## 7. 철회·복구·저장 장애

grant owner는 epoch 갱신과 deny/tombstone, cleanup outbox를 한 durable transaction으로 저장한다.
그 뒤 in-flight cancellation, projection/cache/context 제거, provider credential cleanup을
재시도한다. local vault와 mirror 또는 Go/provider 사이 distributed atomicity를 가정하지 않는다.

- 성공 응답은 최소 durable deny가 commit된 뒤에만 보낸다. cleanup 실패는 `cleanup_pending`이고
  접근은 계속 닫힌다. provider revoke API 실패가 local authority를 되살리지 않는다.
- deny를 저장하지 못하면 성공이라고 표시하지 않는다. 현재 process는 관련 admission을 중단하고
  storage unavailable 상태로 둔다. 시작 시 authority/journal 검증 실패도 fail closed다.
- 새 grant 활성화는 필수 record가 준비되기 전까지 `needs_review`/pending으로 남긴다.
- 오래된 sync/backup의 active record가 tombstone을 덮어쓸 수 없다. replica timestamp로
  last-write-wins authorization을 구현하지 않는다.
- source retention cleanup과 audit 보존을 분리한다. log에는 failure kind, stage, scope handle,
  epoch mismatch 종류, request/invocation correlation만 기록하고 payload/credential은 남기지 않는다.

remote revoke는 owner commit과 producer 적용을 구별한다. 단절 중에는 “모든 기기에서 즉시
삭제됨”을 표시하지 않는다. remote execution/output admission은 현재 authority 확인 없이
offline 캐시 lease로 진행하지 않는다. E2E opaque relay의 암호화·키 복구 구현은 S8에서 결정하되
확정 전에는 해당 protected path를 enable하지 않는다.

## 8. Typed failure와 UI 계약

실패 envelope는 `kind`, `stage`, affected opaque connection/grant reference,
`retryability`, `recovery_action`, correlation ID를 제공한다. source와 session 오류를 분리한다.

| Kind | Recovery | 자동 처리의 상한 |
| --- | --- | --- |
| `access_paused` / `access_revoked` | 접근 설정 | 자동 resume/재동의 금지 |
| `scope_changed` | 현재 범위로 재획득, 필요시 review | 기존 동의를 확대하지 않는 재획득 1회 |
| `connection_replaced` | 연결 선택/재인증 | 다른 계정/provider로 자동 fallback 금지 |
| `observation_expired` | 동일 허용 범위 refresh | deadline 내 1회 |
| `source_unavailable` / `credential_expired` / `rate_limited` | 나중에 재시도/인증 | descriptor TTL 내 허용 cache만 사용 |
| `partial_coverage` | 범위 축소/페이지 추가/불확실성 표시 | 누락을 빈 데이터로 해석 금지 |
| `authority_unverifiable` | owner/storage 연결 복구 | 해당 capability deny |
| `conversation_conflict` | saved session 재조회 | source나 grant 변경 금지 |
| `precondition_failed` | 재판단/새 승인 | 쓰기 replay 금지 |
| `execution_unknown` | 기존 attempt 결과 조회 | 새 action 생성 금지 |

오류 mapping은 versioned DTO로 전환한다. 기존 `stale_context`를 모든 connector의 권한 철회로
번역하지 않는다. optional tool failure는 artifact에 표현할 수 있지만 model에 전달되는
diagnostic에도 raw IDs나 source content를 포함하지 않는다.

## 9. 전환과 downgrade

1. **Inventory:** 모든 `revision` producer/consumer, legacy grant/registry, cached View, action
   approval과 protocol version을 분류한다. migration은 사본으로 검증하며 live user data를
   설계 작업 중 읽어 고치거나 초기화하지 않는다.
2. **Schema:** 새 authority/observation 필드를 명시적 version으로 추가한다. counter 누락을
   `0` 또는 현재 값으로 자동 보완해 authorize하지 않는다. unknown schema는 typed unsupported다.
3. **Connection:** 기존 local mirror revision은 mirror row/write version으로 보존한다. server
   revision은 검증된 server source epoch seed로만 사용할 수 있다. 서로 복사하거나 합산하지
   않는다. source identity/owner가 불명확한 legacy record는 재연결 전까지 deny한다.
4. **Grant:** encrypted durable binding/receipt, active state, 정확한 source identity, 동의 범위·용도·
   consumer/processing policy를 모두 검증할 수 있을 때만 같은 범위의 새 grant로 이관한다.
   sync revision 차이만으로 동의를 확대하지 않는다. 기존 기록이 사용자 동의를 증명하지 못하면
   `needs_review`다. `all`은 확인된 explicit resource set으로 보존하며 dynamic all로 바꾸지 않는다.
5. **In-flight:** legacy lease와 source-derived resumable context를 무효화한다. 미완료 action의
   idempotency/unknown outcome ledger는 보존하고, legacy approval을 현재 version으로 rewrite해서
   실행하지 않는다. 새 validation/approval과 기존 attempt reconciliation을 구분한다.
6. **Commit/recovery:** authority store별 migration marker와 idempotent checkpoints를 저장한다.
   mirror/vault가 모두 준비되기 전에는 protected capability를 열지 않는다. crash 후 재개하며
   verified backup과 이전 epoch/tombstone을 보존한다.
7. **Compatibility:** producer/consumer가 새 계약을 협상한 connection만 enable한다. v1 fallback으로
   검증을 우회하지 않는다. 최소 reader/writer version을 검사하는 bridge build를 먼저 배포한다.
   기존 reader가 새 필드를 무시해도 열 수 있는 DB에 version 숫자만 추가해서 downgrade가 차단된다고
   가정하지 않는다. 새 store format/분리된 credential namespace와 지원 version gate를 실제 이전
   binary로 검증한 뒤 활성화한다. 무단 binary downgrade는 지원하지 않으며, rollback은 새
   deny/tombstone을 이해하는 bridge build 또는 explicit offline recovery로 제한한다.

## 10. 구현 순서와 인수 기준

하위호환 요구는 철회되었다. §9의 legacy migration 및 이전 binary 지원은 구현 범위에서 제외한다.
기존 데이터 삭제는 별도 사용자 동의 없이 수행하지 않는다. 그 외 권한/철회/검증 요구는 유지한다.

설계 단계에서 runtime behavior나 acceptance count를 올리지 않는다. 각 단계는 독립적인 테스트와
coherent commit으로 진행한다.

| 단계 | 범위 | 완료 조건 |
| --- | --- | --- |
| A — contract/reproduction | Calendar sync mismatch 재현, typed stamps/selector/error fixtures, migration inventory | 현 regression과 Rust/Go 공통 deny 조건이 테스트에 존재 |
| B — local authority | Core grant store, epoch/fence/cleanup, native permission adapter | UI 없이 pause/revoke/late-response/crash 조건 통과 |
| C — Calendar vertical slice | mirror version 분리, subset projection, pinned lease, tool-time dispatch, recovery UI | sync 반복·4개 초과 연결·선택 subset·일반 채팅·실제 철회 검증 |
| D — server connectors | Go owner verification, exact connection selection, Mail/Calendar부터 Work/Logistics 순으로 적용 | arbitrary paired request가 grant 검증을 우회하지 못함; cross-language protocol parity |
| E — personal context | Contacts, Wellbeing, Attention/Feasibility에 기존 retention/presence 적용 | raw egress 없음, expiry·device mismatch와 lineage 테스트 |
| F — lifecycle integration | action admission/unknown outcome, resume/Learner lineage, upgrade/rollback | 재시작·철회·불확실 실행 중복 방지 corpus 통과 |

cross-device source relay/owner handover는 S8에서 위 contract를 사용하되 B–F의 local/server 검증을
대신하지 않는다. 단계별 enable gate를 두고 보호되지 않은 connector를 이름만 이관해 켜지 않는다.

### 필수 conformance matrix

| Scenario | Expected result |
| --- | --- |
| 동일 데이터 sync 100회, heartbeat, 정상 token refresh | source/access epoch 불변; observation/health만 갱신 |
| inference 중 새 observation 발행 | 유효한 pinned View 유지, mixed payload 없음 |
| grant 2개 resource, connection 11개 resource | 허용된 2개만 노출; 4개 한도는 authorization failure 아님 |
| scope 축소/확대, dynamic membership, explicit all 재연결 | 침묵 확대 없음; 미지원 selector deny |
| 같은 provider의 다른 account/tenant/connection/device | identity mismatch deny, 자동 대체 없음 |
| token 만료와 실제 permission revoke | 다른 error/recovery; revoke 뒤 unexpired cache도 차단 |
| revoke 전후 read/model egress/artifact/final output의 각 race | fence 뒤 새 release 차단; 이미 전송된 부분은 명시적 한계 |
| 철회 뒤 늦은 publish/callback, retry, resume, summary, learner | tombstone/lineage로 재사용 차단 |
| 일부 source 실패, pagination 중 데이터 변경 | partial coverage 보존, 빈 결과 조작 없음 |
| snapshot expiry, clock rollback/skew, process restart | freshness 연장 없음, legacy lease 재사용 없음 |
| generic chat 중 Calendar 불능 | 일반 대화 가능; 필수 일정 질문은 unavailable 설명 |
| unrelated item 변경 vs action target/availability 변경 | 전자는 허용 가능, 후자는 conditional validation/review |
| action dispatch 전후 revoke, crash after provider success, timeout | 중복 실행 없음; unknown은 lookup/reconcile |
| grant mutation/cleanup/migration 중 crash, stale backup replay | durable deny 우선, active resurrection 없음 |
| old/new mixed client-server, missing stamp, forged permit | protected path fail closed, 일반 chat은 가능한 범위에서 유지 |
| 다른 Person 또는 revoked paired device의 request | source read 이전 거부, raw IDs/content 없는 audit |
| raw Health/activity/location와 allowed derived projection | retention/transfer 규칙과 별도 consent 검증 |
| network partition during remote revoke | authority 확인 없는 새 remote admission 거부, 전체 즉시 철회 주장 금지 |

unit/fixture 통과는 실제 OS revoke·provider conditional write·remote privacy 검증을 대체하지
않는다. live test는 disposable resource와 명시적 consent로 별도 evidence에 남긴다.

## 11. 구현 전 남은 결정

- source-specific subject proof와 resource selector의 정확한 의미/범위 한도.
- remote grant owner verification protocol, trusted issuer enrollment와 anti-replay contract.
- protected history·Memory의 보존/재동의 세부 UX, provenance 없는 legacy context의 처리 화면.
- snapshot pinning의 per-Person memory quota와 connector별 stable pagination 지원 여부.
- authority incarnation recovery와 S8 handover/clock-skew/opaque relay protocol.

미결정인 기능은 unsupported 또는 needs review로 남긴다. default allow, 자동 scope 확장,
TTL만으로 remote revoke를 보장하는 fallback은 사용하지 않는다.
