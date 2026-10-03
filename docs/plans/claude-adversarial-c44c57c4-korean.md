# Claude adversarial review: second response

Local Claude Code 2.1.287, requested claude-opus-5-5, effort high, exit 0 reported. Input c44c57c4879c38034d82a040fad22ba43a606e7c only; current S1 not reviewed. Raw Korean response below. The first Russian response is retained separately because the findings conflict in places; both require source-grounded native adjudication.

---

# Floe 전체 구조 전환 계획 + T0 근거: 적대적 정적 검토

**범위:** archive `c44c57c`(baseline `3f4b407`)에 있는 계획 문서, T0 ledger, 실제 소스와 manifest만 정적으로 읽었습니다. 명령, build, 테스트는 하나도 실행하지 않았고 파일도 수정하지 않았습니다. 진행 중인 cloud S1 구현은 이 archive에 없으므로 검토하지 않았습니다.

**결론:** Blocker나 High로 판단한 finding은 없습니다. Medium 3건과 Low 2건을 찾았고, 모두 취소·외부 효과가 불확실해진 뒤의 복구와 credential·identity 경계에 몰려 있습니다. R4와 R8은 partial로 평가합니다.

---

## Findings (영향이 큰 순)

### F1. Medium · 확립된 결함 — Actions의 Update/Delete 복구 계약을 현재 native로는 구현할 수 없음

- **계약:**
  - `docs/plans/2026-10-02-app-cutover.md:223`은 `CalendarEffect`를 Create/Update/Delete로 정의합니다.
  - 같은 파일 `:248`은 `Failed`에 저장된 `NotAppliedProof`를 요구합니다.
  - `:114`, `:253`은 `reconcile`이 "exact marker/idempotency lookup"으로 불확실성을 해소한다고 씁니다.
  - `docs/plans/2026-10-02-client-cutover.md:138`은 `EventKitActions.swift`를 KEEP하고 "DTO cutover만 적응"하라고 합니다.
- **실제 코드:**
  - `apps/client/macos/CalendarActions/EventKitActions.swift:410-415`: lookup에서 `guard !proposal.deleting ... else { throw NativeFailure("uncertain_result") }`입니다. 삭제는 lookup이 항상 실패합니다.
  - 수정도 적용되지 않았다면 `proposal.matches(event)`가 거짓이 되어 역시 `uncertain_result`입니다.
  - 오류는 `:482-486`의 문자열 하나뿐이고, 적용되지 않았다는 증거를 전달하는 채널이 없습니다.
  - Rust `crates/adapters/providers/src/sources/native_calendar.rs:570-577`은 별도 스레드에서 `recv_timeout(15s)` 후 `Timeout`을 반환합니다. 이때 native 스레드는 계속 실행되어 `store.remove`를 끝낼 수 있습니다.
- **정적 반례:**
  1. 사용자가 승인한 Delete 액션이 `prepare_dispatch` 후 Executing 상태가 됩니다.
  2. EventKit 응답이 15초를 넘어 Rust는 `Timeout`을 받고 `Unknown`이 됩니다.
  3. `reconcile`이 `lookup`을 호출하면 `:412`에서 `uncertain_result`가 나와 다시 `Unknown`입니다. 이 상태는 영원히 반복됩니다.
  4. `pending_recovery`(app-cutover `:238`)가 매번 이 레코드를 다시 꺼냅니다.
  5. `Failed{NotAppliedProof}`는 이 코드 경로에서 아예 생성될 수 없습니다.
- **기대 vs 실제:** 계획은 "reconcile resolves uncertain outcomes"를 약속합니다. 실제로는 삭제의 경우 구조적으로 해소가 불가능하고, Failed/Unknown 구분도 구현할 수 없습니다. `vault-cutover.md:159`의 "unknown remains unknown"이 이 결과를 묵인할 뿐, 종결 경로는 정의되어 있지 않습니다.
- **권고:**
  - 효과 종류별 reconcile 증거를 정의합니다. Delete는 결과를 단정하지 않는 사용자 확인 종결 상태(예: `UnknownAcknowledged`)가 필요합니다.
  - native ABI에 "write 이전 실패" 단계를 표현하는 타입을 추가합니다.
  - 아니면 S2.4 범위에서 Update/Delete를 명시적으로 제외하거나 보류합니다.
  - `pending_recovery`의 반복 lookup에 상한을 둡니다.

### F2. Medium · 확립된 누락 — reset 전 hazard check에 소유자·단계·실행 수단이 없음

- **계약:**
  - `architecture-refactor.md:497,499`와 `app-cutover.md:185,269`는 "reset 전에 in-flight/uncertain 외부 효과를 좁게 확인하라"고 합니다.
  - 동시에 `:499`는 old-schema decoder를 금지하고, `:374`(S1.3)는 "old local profiles fail explicitly"라고 합니다.
  - `root-tools-cutover.md:29`는 reset 스크립트를 REWRITE하라고 하지만, hazard check 요구는 들어 있지 않습니다.
- **실제 코드:** `scripts/reset-local-data.sh:32-37`은 `pgrep`으로 프로세스 실행 여부만 봅니다. `:55-68`은 `com.floe.agent-vault.v1`을 포함한 Keychain 항목을 영구 삭제합니다. 그러면 Trash에 옮긴 암호화 DB도 더 이상 읽을 수 없습니다.
- **정적 반례:**
  1. 기존 개발 profile에 `agent_action_envelopes` 행(`vault/agent_actions.rs`)이나 평문 Turso action projection이 Executing 상태로 남아 있습니다. EventKit에는 이미 생성됐을 수 있습니다.
  2. S1/S2가 적용된 런타임은 설계상 old schema를 읽지 못합니다.
  3. reset 스크립트에는 검사가 없고, 키를 지우면 이후 검사 자체가 불가능해집니다.
  4. 따라서 hazard check를 수행할 수 있는 시점(baseline 바이너리가 살아 있는 동안)을 계획 어디에서도 지정하지 않습니다.
- **영향:** Floe marker가 붙은 외부 캘린더 쓰기가 기록 없이 남습니다. 사용자가 다시 요청하면 새 execution/marker로 중복 생성될 수 있습니다.
- **권고:**
  - "S1 cutover 전, baseline 런타임으로 non-terminal Action을 읽기 전용으로 열거하고 결과를 기록"하는 단계를 T0/S1 선행 조건으로 명시합니다.
  - 재작성할 reset 스크립트가 그 기록 없이는 진행을 거부하도록 요구합니다.
  - 이 리뷰에서는 reset을 실행하지 않았습니다.

### F3. Medium · 미검증 가설 — 원격에서는 commit됐지만 로컬은 미commit인 pairing에 종결 경로가 없음

- **계약:**
  - `architecture-refactor.md:138`은 "Lost acknowledgement rejoins the same pairing ID"와 "RepairRequired"를 정의합니다.
  - `server-cutover.md:73`: Go는 평문 앱 토큰을 메모리에만 들고 있습니다.
  - integration cancel은 `canonical-contracts.md:278`에 "remote completion won"으로 정의되어 있습니다. 그러나 pairing cancel이 원격 activation과 경합하는 경우는 어디에도 정의되어 있지 않습니다.
  - `forget_gateway`(`canonical-contracts.md:234`)는 `gateway_ref`와 revision을 요구하는데, 한 번도 Paired에 도달하지 못한 operation에는 둘 다 없습니다.
- **baseline 근거:** `server/internal/pairing/pairing.go:250-277`에서 토큰은 hash만 영속화되고 평문은 `pending.token`에만 있습니다. `:193-200`의 poll은 그 메모리 값을 재전달합니다.
- **반례 3가지:**
  - (a) Go `ActivatePairing` commit 후 Go 프로세스가 재시작되면 토큰이 사라집니다. Rust의 rejoin은 bearer를 받을 수 없어 RepairRequired가 됩니다.
  - (b) Go 승인 직후 사용자가 `cancelPairing`을 누르면 Rust는 Cancelled가 되지만, Go 쪽 client와 issuer는 active로 남습니다.
  - (c) 로컬 Keychain write/readback이 실패하면 RepairRequired가 됩니다.
  - 세 경우 모두 Rust에서 원격 고아 client를 revoke하거나 재-pairing을 허용하는 계약이 없습니다. 같은 owner key로 재-pairing할 때 "bundle drift"로 거부될 가능성도 있습니다(Go trust 신규 구현은 미확인).
- **권고:**
  - pairing cancel과 activation의 경합 규칙을 정의합니다.
  - RepairRequired 상태에서 허용되는 action을 정의합니다.
  - "미전달 credential의 원격 revoke" 또는 봉인된 재전달 수단을 정의하고, 재-pairing 전 선행 조건으로 둡니다.

### F4. Low · 확립된 계약 공백 — 기존 slot 네임스페이스는 유지하면서 payload는 바꾸는데, 깨진 slot을 정리할 제품 경로가 없음

- **계약:** `architecture-refactor.md:134,136`은 기존 `app.floe.local-server`/`connection-v1` slot을 유지하라고 하면서 payload에 pinned producer와 revision을 추가합니다. 읽을 수 없는 값은 "absence가 아님"으로 처리합니다(`canonical-contracts.md:112`).
- **baseline 근거:** 기존 slot은 5필드 스키마입니다(`crates/adapters/providers/src/control/server_connection.rs:10-22`). Flutter도 같은 slot에 씁니다(`apps/client/macos/Runner/MainFlutterWindow.swift:61-62`, `apps/client/ios/Runner/AppDelegate.swift:54-55`).
- **반례:** cutover 전에 pairing한 개발 머신에서는 새 store가 malformed 오류를 냅니다.
  1. `observe_primary`가 Err가 되어 모든 turn이 실패합니다. fallback으로 넘어가지 않으므로 이 동작 자체는 안전합니다.
  2. 그러나 `gateway_ref`가 없어 forget할 수 없습니다.
  3. `remove`는 `GatewayCredentialRevision`을 요구합니다.
  4. pairing commit 때 이 slot을 교체할 `CredentialExpectation`이 정의되어 있지 않습니다.
- **판단:** fail-closed이고 clean-profile reset이 수용된 해결책입니다. 다만 계획이 "이 slot은 reset 대상"이라는 점을 명시하지 않았습니다.
- **권고:** account를 `connection-v2`처럼 명시적으로 bump하거나, "읽을 수 없는 slot 교체"를 위한 기대값과 repair 명령을 정의합니다.

### F5. Low · 미검증 가설 — Person ID 고정 상수가 Swift/Dart에 남아 Rust 쪽 정리와 모순됨

- **계약:** `architecture-refactor.md:58`과 `rust-cutover.md:62`는 Rust의 `LOCAL_PERSON`을 삭제하고 "no replacement magic constant"라고 합니다.
- **실제 코드:**
  - `EventKitActions.swift:6`이 `localPerson` 상수를 선언하고 `:78,217,303`에서 동등 비교로 검사합니다. 이 파일은 KEEP입니다(`client-cutover.md:138`).
  - `apps/client/lib/app/local_identity.dart:1`도 KEEP입니다(`file-read-ledger/client.json:2903-2918`).
  - `main.dart:71,89,107`이 이 상수를 그대로 broker에 넘깁니다.
- **반례:** clean profile이 상수가 아닌 PersonId로 생성되면, Rust가 넘기는 검증된 ID가 Swift `:78`/`:303`에서 `permission_denied`로 거부되어 EventKit 전체가 막힙니다. 반대로 상수가 유지되면 "verified host identity"는 결국 그 고정 상수입니다. App §2.1은 profile의 Person ID 생성 규칙을 정하지 않습니다.
- **권고:** 단일 Person 상수를 제품 불변식으로 문서화하거나, 검증된 Person을 native 요청으로 전달하고 Swift의 동등 비교를 제거합니다.

---

## 반증을 시도했지만 결함이 아니었던 지점

- **의존성 DAG (R7):** 실제 manifest(`crates/modules/{inference,connections,conversation,context,experts,knowledge,actions}/Cargo.toml`, `crates/adapters/{vault,providers}/Cargo.toml`, `crates/bindings/ffi/Cargo.toml`)와 `canonical-contracts.md:362-406`를 대조했습니다. 일치하며, Inference와 Connections 사이에 도달 경로가 없습니다. Vault→native 간선이 제거된 것도 확인했습니다.
- **Learner의 Gateway 전환 (S1) 유출 가능성:** baseline이 `DependencyCoverage::Independent`만 허용하므로(`crates/modules/knowledge/src/application/memory.rs:150`, `crates/adapters/vault/src/vault/learning.rs:662-669`) source 증거가 Gateway로 새로 나가는 경로는 없습니다.
- **Access commit 안의 Actions 무효화:** 소유권 위반을 의심했으나, vault-symbol-map(`:67443-67545`)이 Actions의 순수 전이 `invalidate_for_grant`로 분리해 두었습니다. 결함이 아닙니다.
- **R5 원래 반례 표본:** `legacyGateway` DELETE, `stash_blocked` DELETE, `record_result_independent`→Context MERGE 모두 정정됐습니다.
- **T0 등록 수:** Rust 751+335+142+70=1298, Go 184+33+49=266(`t0-go-coverage-audit.json:620-630`)로 합계가 맞습니다. 표본으로 본 `learner_worker.rs` inline 테스트도 App ledger에 기록되어 있습니다.

---

## R1–R8 평가

| ID | 평가 | 근거 |
|---|---|---|
| R1 | closed (정적 spec) | `canonical-contracts.md:13-83,104-112,176-197`과 main `:186-193`이 일치합니다. absence는 strict inventory나 로컬 slot 읽기 성공에서만 나옵니다. |
| R2 | closed | `vault-cutover.md:102-137`. Blocked의 terminal 처리, 단일 child slot, 사용자 텍스트 미복제가 정의되어 있습니다. 반례를 찾지 못했습니다. |
| R3 | closed | `vault-cutover.md:44,92-96`. `GrantOperationSlot`이 commit과 abort를 상호 배제하고, Vault가 잠기면 fence를 유지합니다. 반례를 찾지 못했습니다. |
| R4 | **partial** | integration cancel은 정의됐지만(`canonical-contracts.md:278`), pairing cancel과 승인의 경합, RepairRequired 종결이 없습니다(F3). |
| R5 | closed (원래 반례 기준) | 위 표본 기준입니다. 새로 생긴 prose/map 긴장 1건은 F5입니다. |
| R6 | closed | `client-cutover.md:169`와 central S1 순서가 일치합니다. G1 명령(`architecture-refactor.md:439-446`)도 합의된 범위입니다. |
| R7 | closed | 위 manifest 대조 결과입니다. |
| R8 | **partial** | 저장소 일원화와 소유권은 닫혔습니다. 그러나 Update/Delete 복구와 `NotAppliedProof` 생산자가 없고(F1), 레거시 hazard check 소유자도 없습니다(F2). |

---

## 검토하지 않은 영역

- Go trust/authority/integrations의 신규 내부 설계와 OAuth connector 감사 문서
- bindings ABI/header 세부
- client presentation/feature UI 계획
- Health Swift transform 계약과 실제 코드의 대조
- 대부분의 T0 per-assertion 내용 (표본만 확인)
- Rust Engine journal과 budget 변경 세부
- cloud S1 구현 (archive에 없음)

