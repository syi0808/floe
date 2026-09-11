# Connection, Access & Observation

> Status: Proposed common contract — 2026-09-12
>
> Scope: 모든 data connector의 공통 의미. 이 문서는 런타임 구현 완료를 뜻하지 않는다.

## 목적과 경계

Connection의 정상 동기화가 AI 접근 권한을 무효화하지 않으면서, 실제 권한 철회는
캐시·진행 중 작업·재시도를 통해 우회되지 않도록 한다. Calendar 전용 예외가 아니라
Mail, Contacts, Health, Attention, Work, Logistics와 향후 파일 connector에 적용한다.

기존 [DataAccessGrant](../01-experience/data-access-and-processing.md),
[Observe/Act/Interact](connector-contract.md),
[Person-owned connection](../../decisions/0025-person-owned-connections.md),
[observation envelope와 query lease](../../decisions/0024-device-context-collection-and-convergence.md)를
확장한다. 새로운 병렬 permission 체계를 만들지 않는다. Interact와 inference resource에도
identity/authority/health 분리는 적용하지만, data retention이나 DataAccessGrant를 그대로
강제하지 않는다. 출력 채널과 외부 모델 전송에는 각자의 별도 동의가 필요하다.

구체적인 저장·검증·전환 계획은
[Connection authorization runtime](../09-implementation/connection-authorization-runtime.md)에 둔다.

## 1. 불변 조건

1. 연결돼 있고 credential이 유효하다는 사실만으로 AI 사용을 허용하지 않는다.
2. 정상 관측·토큰 갱신·health heartbeat는 사용자 동의를 변경하지 않는다.
3. 허용 범위의 합집합으로 부족한 권한을 보충하지 않는다. 모든 정책의 교집합만 사용한다.
4. 관측 결과는 evidence이며 실행 권한이 아니다. Observe, Act, Interact는 별도다.
5. provider 종류, display name 또는 최신 timestamp만으로 계정·기기를 선택하지 않는다.
6. 권한 상태를 검증할 수 없는 경우 해당 기능을 닫는다. 정상 일반 대화까지 닫지는 않는다.
7. UI, Manager, Expert는 authority stamp나 사용자 동의를 직접 발급·갱신하지 않는다.
8. 철회 뒤 늦게 도착한 응답, 예전 snapshot, history, retry가 권한을 부활시키지 않는다.

## 2. 세 가지 별도 모델

### Connection: 실제 접근 가능한 source

```text
Connection {
  person_id, connection_id, connector_id
  source_identity_ref
  execution_owner
  credential_ref?
  selected_source_scope
  source_epoch
  lifecycle
}
```

- `connection_id`는 opaque identity다. 같은 provider의 여러 계정·연결을 구별한다.
- `source_identity_ref`는 provider subject/tenant 또는 OS store identity에 대한 private
  reference다. 이메일 주소나 화면 이름으로 동일성을 증명하지 않는다.
- `execution_owner`는 device ID 또는 server instance ID다. device ID를 OS 이름으로 대체하지
  않으며, Go server는 ADR 0026대로 자기 credential을 소유한다.
- `selected_source_scope`는 Floe에 연결한 resource 범위다. provider OAuth scope와 AI scope를
  구분한다. OS/provider의 현재 접근 가능 범위는 실행 owner가 검증한다.
- Person, source account/tenant, execution owner를 다른 정체성으로 교체할 때는 새 connection을
  만든다. 기기 재등록·server 이전도 기존 동의나 미완료 action을 자동 이전하지 않는다.
- 같은 계정의 정상 token refresh는 identity 변경이 아니다. 재인증에서 같은 subject를
  증명하지 못하면 기존 connection을 재사용하지 않는다.

Connection의 health, last success, last failure, retry time은 별도 관측 상태다. `offline`,
`rate_limited`, 일시적 token 만료를 곧바로 사용자 동의 철회로 바꾸지 않는다. 반대로 실제
permission denial은 health 경고만 남기고 cached access를 계속 허용해서는 안 된다.

### DataAccessGrant: 사용자가 AI에게 허용한 사용

```text
DataAccessGrant {
  person_id, grant_id, connection_id
  authority_owner
  domain, resource_selector, data_categories, allowed_uses
  allowed_consumers
  processing_policy_ref
  status, access_epoch
  granted_at, revoked_at?
}
```

- grant는 정확히 한 connection에 속한다. 여러 connection을 쓰는 Expert는 각각의 grant를
  검증한다. 화면의 domain card는 여러 grant를 묶어 보여줄 수 있다.
- `allowed_uses`는 read, suggestion 등 의미상의 용도다. mutation authority나 외부 모델
  전송 권한을 암묵적으로 포함하지 않는다.
- `allowed_consumers`는 허용된 built-in/extension 또는 그 정책 참조다. 설치만으로 grant의
  모든 데이터를 받지 않는다. Expert의 선언 권한과 현재 assignment도 별도로 교차 검증한다.
- `authority_owner`는 해당 grant의 변경을 직렬화하는 단일 정책 owner다. 여러 replica가
  동시에 자신의 counter를 늘려 동의를 합치지 않는다.
- `active`, `paused`, `revoked`, `needs_review`를 구분한다. `revoked`는 terminal이고 재허용은
  새 grant다. `paused`의 명시적 resume은 같은 범위에서 새 epoch를 발급한다.
- source가 offline이면 grant는 active여도 실행이 unavailable일 수 있다. 연결 복구와
  사용자 동의 변경을 같은 UI action으로 취급하지 않는다.

### ContextObservation: 실제로 관측한 bounded evidence

ADR 0024의 envelope에 정확한 connection identity와 source authority stamp를 추가한다.
대화에서 사용한 `snapshot_id`의 역할은 기존 `observation_id`가 맡는다. 같은 내용을 뜻하는
두 public ID를 새로 만들지 않는다.

```text
ContextObservation {
  observation_id, person_id, connection_id, source_epoch
  logical_source, view_id, view_version
  producer, scope_handle
  observed_at, expires_at, source_revision?
  coverage, provenance[], retention, sensitivity, transfer
  payload
}
```

- 발행된 envelope와 payload는 immutable하다. 새 동기화는 새 observation을 발행한다.
- 내용이 같아 storage blob을 재사용해도 새 관측 시각은 새 envelope다. 단순 cache read는
  `observed_at`이나 expiry를 연장하지 않는다.
- 일부 source만 새로 읽었으면 source별 시각·coverage를 유지한다. aggregate 발행 시각으로
  오래된 부분을 fresh하게 만들지 않는다.
- payload 저장 정책은 기존 [retention class](connector-data-policy.md)를 따른다. snapshot
  pinning을 도입해도 raw Health, location, activity의 durable history를 만들지 않는다.
- provider cursor, ETag, message/event revision은 opaque provider metadata다. permission
  counter나 서로 다른 connector의 최신성 순위로 사용하지 않는다.

## 3. 버전의 의미

| 값 | 변경 주체와 조건 | 비교할 곳 |
| --- | --- | --- |
| `source_epoch` | execution owner: source scope, 검증된 provider 권한, disconnect/revoke/reconnect 경계 변경 | observation 획득·사용 및 source 실행 |
| `access_epoch` | grant authority: AI 범위·용도·consumer 변경, pause/resume/revoke | grant를 사용하는 모든 작업 |
| `policy_epoch` | processing/consumer/action 정책 owner: recipient·허용 category·Expert 권한·실행 정책 변경 | 해당 consumer/processing/action 경계 |
| `observation_id` | producer: 새 bounded 관측 발행 | 한 작업에서 사용하는 evidence 고정 |
| `row_version` | repository: 개별 record 갱신 | 그 record의 optimistic write/CAS |
| `source_revision` | provider: provider 자체 계약 | delta sync, item conditional write |

권한 stamp는 단일 숫자가 아니라 정확한 identity에 속한 epoch들의 묶음이다. `source_epoch=7`과
다른 connection의 `7`은 관련이 없다. `access_epoch`와 `source_epoch`끼리 같은 숫자일 필요도
없다. counter는 영속적이며 재시작·복구로 감소하거나 재사용하지 않는다. overflow는 오류다.
identity나 counter의 연속성을 증명할 수 없는 복원은 새 authority incarnation과 재검토를
요구하며, 이미 발급한 lease를 되살리지 않는다.

## 4. 변경별 반응

| 사건 | Source authority | AI grant | 진행 중 작업 |
| --- | --- | --- | --- |
| 일정·메일·문서 갱신, 정상 sync | 유지 | 유지 | pinned evidence가 유효하면 계속 |
| 동일 subject/scope의 token refresh | 유지 | 유지 | 필요시 transport만 재시도 |
| rate limit/offline/일시적 credential 만료 | 유지, health 변경 | 유지 | 허용된 unexpired cache 또는 typed unavailable |
| source 목록에서 선택 범위 변경 | 새 source epoch | 자동 확장 안 함 | 이전 lease 종료, 현재 교집합으로 다시 획득 |
| AI grant 범위 축소 또는 pause | 유지 | 새 access epoch | 이전 grant 의존 작업 무효화 |
| AI grant 확대·새 consumer 허용 | 유지 | 명시적 동의 후 새 access epoch | 현재 grant로 새 작업만 시작 |
| OS/provider permission 철회 | 새 source epoch, 접근 차단 | 동의 기록과 실제 가용성을 구분 | cached result 포함 관련 작업 차단 |
| 사용자 grant 제거 | 유지 가능 | revoked/tombstone | 관련 작업·resumable context 차단 |
| connection 해제·계정/owner 교체 | 종료/tombstone, 새 연결은 새 ID | 자동 승계 금지 | 기존 target의 작업과 응답 차단 |
| 모델 recipient 변경 | 유지 | 참조 policy 재검토 | 새 recipient로 전송 금지 |

source epoch 변경은 이전 실행 허가를 무효화하지만 grant의 resource set을 수정하는 동의가
아니다. source 범위 축소 뒤에도 유효한 subset은 현재 정책으로 새롭게 투영할 수 있다.
단, 이전의 더 넓은 evidence를 모델이 이미 사용했다면 그 모델 context와 미공개 결과를
재사용하지 않고 새 context로 다시 시작한다.

## 5. 범위는 identity 기반의 교집합

```text
effective_scope = current provider/OS authority
                ∩ connection selected scope
                ∩ active DataAccessGrant selector
                ∩ consumer declared/assigned permissions
                ∩ purpose and processing policy
```

`resource_selector`는 정렬·중복 제거된 opaque resource reference의 명시적 집합을 기본으로 한다.
provider-native ID의 실제 해석은 connector private state에서 한다. 범위 비교·동일성·projection은
domain adapter가 정의하고, 알 수 없는 selector는 fail closed다. 자유형 LLM query나 문자열
prefix 비교를 권한 검사로 사용하지 않는다. Mail label/folder, Drive folder의 재귀·이동·shortcut,
contact group membership처럼 동적인 selector도 포함 범위를 명시해야 한다.

초기 구현은 `explicit_resources`만 허용한다. “현재 전체 선택”도 확인 당시 resource 집합을
저장한다. “향후 추가 resource도 포함”하는 dynamic selector는 별도의 reviewed scope 문구와
connector conformance가 준비되기 전까지 unsupported다. 기존 Calendar `scope=all`만으로
미래 resource에 대한 AI 동의를 추정하지 않는다.

연결된 캘린더가 11개이고 AI 허용 범위가 2개라면, Expert에는 그 2개에서 만든 View만 준다.
UI mirror용 수집 범위와 AI projection 범위는 달라도 된다. item/byte/resource limit은
projection의 budget이지 권한 철회가 아니다. 초과 시 bounded pagination 또는 명시적 partial
coverage를 반환하며, 처리하지 않은 부분을 “일정 없음”이나 “모든 메일 처리됨”으로 표현하지
않는다. limit을 넘었다고 전체 grant를 지우거나 관측 불능을 permission revoked로 보고하지 않는다.

## 6. 기능별 최신성과 action 조건

기본 freshness profile은 ADR 0024를 유지한다. consumer는 더 엄격하게 요구할 수 있지만
source expiry를 연장할 수 없다. `no_data`는 성공적으로 확인한 빈 범위에만 사용한다.

| Domain | 공통 규칙 위의 추가 의미 |
| --- | --- |
| Calendar | 시간 범위·timezone·coverage를 검증. mutation은 대상 item revision과 필요시 busy/free 조건 재검증 |
| Mail / messages | 목록·metadata와 body 접근 category를 구분. 목록 snapshot이 나중의 body read 권한을 보장하지 않음. draft/send는 별도 Act |
| Contacts | identity reference만 유지. 수신인·전화번호를 사용하는 action은 대상 identity 재확인 |
| Health / Wellbeing | raw read와 local derivation은 producer 내부. derived View의 provenance·category와 별도 transfer 동의 검증 |
| Attention / Location / ETA | device/presence·query binding·짧은 TTL 필수. 작업 고정 때문에 expiry를 넘기거나 history를 보관하지 않음 |
| Files / Work | 문서 이동·ACL·선택 workspace 변경 검증. 변경 대상 version과 실제 resource identity 재확인 |
| Logistics / Home | read-only 상태와 제어 명령을 분리. 물리 action은 connector별 위험·승인·사전조건 요구 |

Floe canonical Task/Note/Memory는 외부 OAuth connection을 억지로 갖지 않는다. 동일한 접근
검증 port를 통해 내부 source identity, record version, Memory policy와 provenance를 사용한다.

## 7. 대화와 복구 경험

Manager는 사용 가능한 capability metadata로 계획하고 실제 source 데이터가 필요한 도구를
호출할 때 권한·freshness를 확인한다. 모든 메시지 앞에서 전체 connector sync를 기다리지 않는다.
optional source 실패는 기능 단위로 보고하고, 필요한 source가 없으면 결론을 꾸며내지 않는다.

- `access_revoked` / `access_paused`: 영향을 받는 데이터와 접근 설정을 안내한다.
- `scope_changed` / `connection_replaced`: 새 범위 확인 또는 재연결을 안내한다.
- `observation_expired` / `source_unavailable`: bounded refresh 또는 나중에 재시도한다.
- `partial_coverage`: 어떤 범위를 확인하지 못했는지 설명한다.
- `conversation_conflict`: 저장된 메시지만 다시 읽는다. source 권한을 변경하지 않는다.
- `execution_unknown`: 기존 action 결과를 조회한다. 새 실행 요청을 만들지 않는다.

새로고침은 동의 변경이 아니다. 입력한 메시지를 preflight 실패로 지우지 않으며, 저장된 user
message와 turn identity로 중복 전송을 방지한다. source 실패만으로 전역 `needsReload`를 설정하지
않는다. vault 잠금·대화 저장 무결성 실패는 별도의 session-level 차단이다.

## 8. 철회의 보장과 한계

authority owner에서 철회가 commit되면 새 lease, 새 egress, 새 action admission을 차단한다.
이미 취소할 수 없는 provider 실행이나 이미 전송·표시된 bytes를 회수했다고 주장하지 않는다.
OS/provider 권한 철회도 관측·검증하기 전까지 즉시 알 수 있다고 보장하지 않는다.

모델이 읽은 evidence의 dependency는 요약·Expert 결과·continuation에도 이어진다. 철회된
evidence를 단순히 다음 prompt에서 빼는 것만으로 이미 생성 중인 응답을 안전하게 만들 수 없다.
관련 model context를 폐기하고, 미공개 출력·proposal·학습 candidate의 재사용을 막는다.
보존된 conversation archive도 AI 재입력 가능 여부와 별도로 관리한다. 사용자가 이미 본 내용을
소급해서 지웠다고 표시하지 않는다. source 기반 파생물의 삭제/보존은 provenance와 도메인
retention 정책으로 처리하며, audit에는 content가 아닌 최소 metadata만 남긴다.

cross-device 철회는 owner의 online 검증 또는 전파 확인 없이는 즉시 보장할 수 없다. 초기 구현은
remote boundary에서 authority freshness를 확인할 수 없으면 거부한다. 서명된 lease의 TTL만으로
즉시 철회를 보장하지 않는다. offline execution 확장은 별도 보안 결정 전까지 지원하지 않는다.
