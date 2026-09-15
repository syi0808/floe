# 09 — Stage A 이후 동작 검증·안정화

**단계:** B. **기존 범위:** P13/P14/P21/P22/P23/P25의 동작 검증. **선행:** 08 구조 심사. 같은 에이전트가 순차 수행하며 제품 검증을 위해 subagent를 만들지 않는다.

이 단계의 번호는 실행 순서다. 아래 테스트 이름은 작성할/재사용할 검증의 **설명 이름**이며 현재 저장소에 모두 존재한다는 뜻이 아니다. 해당 함수·실제 test target을 검색하여 동등한 기존 회귀를 우선 사용한다.

## 09.1 동일 snapshot의 controlled composition과 장애 주입

**읽기 시작:** 08의 구조 결과와 이동 원장, [S05/S26 owner repository](../SOURCE_ANCHORS.md), [S23/S24 Engine](../SOURCE_ANCHORS.md), 최종 app tests·FFI C ABI·Dart runtime client 테스트. 과거 경로가 이동했으면 최종 위치를 사용한다.

1. 실제 HEAD/dirty diff, Rust/Dart/native artifact identity, SDK와 test data 경로를 기록한다. 외부 provider/model만 통제된 대역으로 바꾸고 **실제 owner→repository→FFI→client** 연결은 우회하지 않는다.
2. 저장 형식·의미가 변경되었으면 명시적인 새 Floe 개발 profile로 생성한다. 같은 schema 번호의 과거 DB를 재사용해서 current reopen 검증으로 간주하지 않는다. old file/key 자동 삭제는 하지 않는다.
3. 다음 표를 순서대로 실행한다. 한 실패를 고칠 때 해당 owner만 수정하고 영향받은 구조/안전 검증을 다시 실행한다. 같은 환경의 동일 성공을 각 기능마다 중복 실행하지 않는다.

| 시나리오 | 준비·주입·실행 | 반드시 관측할 결과 / 금지 동작 |
|---|---|---|
| current contract | 같은 snapshot ABI와 Dart decode, old symbol lookup 시도 | current만 성공; alias/fallback 없음, schema 숫자 증가 없음 |
| fresh/reopen | 새 current DB에서 Session/Run/receipt 저장 후 reopen | 같은 ID/결과 복구; 새 키 덮기·오류 자동 reset 0 |
| admission without network | model inventory·catalog 대역을 barrier에 걸고 StartTurn | 로컬 receipt 수락; query/cancel 처리; source 미사용 catalog 호출 0 |
| command after environment change | 첫 수락 후 token/route/catalog/reachability 변경, 같은 command 재전송 | 같은 receipt, 추가 모델/도구 dispatch 0; 다른 payload는 Conflict |
| session claim/terminal | 같은 Session에 두 command, terminal ack 유실·저장 실패 주입 | 두 root 쓰기 금지; confirmed finish 뒤 UI Release 없이 다음 요청; unconfirmed는 해당 Session recovery |
| child failure/root cancel | Expert를 denied/timeout, 별도 실행에서 root Cancel | child 실패 사실+허용된 설명; root cancel 후 추가 dispatch 0 |
| pending batch/journal ack | 모델 batch의 2번째 step 전 저장 실패, Task 결과 ack 유실 | ack 전 effect 0; 저장된 call/Task ID 재조회; 다른 batch를 이전 replay로 처리 안 함 |
| one attempt accounting | queue 취소·handoff 후 응답 유실·usage 재수신 | 미발송과 unknown 구별, 같은 Attempt 비용 한 번만 반영 |
| registry extension | Calendar 활성, scripted Manager가 Communication 선택; 새 test endpoint 등록 | 선택 ID 보존, generic source 수정 0, Schedule도 같은 Task 경로 |
| current release | source-dependent 결과 생성 후 grant revoke/epoch 변경 | 새 remote 전송·UI 공개 0; Unknown을 Independent로 승격하지 않음 |
| Connect during chat | model answer barrier 중 Query/Preview/Refresh | query/preview 끝남, chat stop 0; refresh 실패가 disconnect 아님 |
| OAuth/uncertain mutation | 화면 dispose/reentry, 늦은 operation 응답, remote ack 유실 | 같은 operation 관측; Disconnect 반전 0; 새 ID의 blind write 0 |
| transport/events | worker exit, close 중 pending, cursor gap, 늦은 epoch event | waiter 한 번 settle; 화면 종료 cancel 0; durable snapshot 복원 |
| whole canonical path | Vault→Session→답변→두 번째 입력→Connect→취소→재진입 | 모든 기능이 같은 현재 API, old bridge 호출 0 |

4. effect counters는 대역의 실제 dispatch 경계에 둔다. UI fake만의 call count로 제품 전체의 금지 호출 0을 주장하지 않는다. 실서버 mutation을 테스트하려면 별도 허용된 환경을 사용한다.
5. 넓은 suite 실패는 구현 버그/계약 결함/환경/구형 API 가정으로 분류한다. 현재 권한 보호 실패를 오래된 테스트라고 쉽게 제외하지 않는다. 바뀐 의미의 근거를 기록한 뒤 필요한 테스트만 수정한다.

```bash
cargo test -p floe-conversation
cargo test -p floe-agent-runtime
cargo test -p floe-experts
cargo build -p floe-ffi
(cd apps/client && flutter test)
(cd server && go test ./... && go vet ./...)
```

실제 target·SDK를 확인하고 적용한다. 모든 명령의 성공을 미리 가정하지 않는다.

## 09.2 일반 Apple 앱의 Vault/Keychain 문제 확인

**기준 관측:** `89452eb`까지의 과거 기록은 일반 macOS debug 앱이 route 선택 전에 VaultUnavailable로 끝났다고 보고한다. 원인은 확정되지 않았다. 이 관측은 새 코드에서 반드시 같은 실패가 난다는 뜻이 아니다.

**원본 위치:** [S29 keyring](../SOURCE_ANCHORS.md#s29), [S27 vault lifecycle](../SOURCE_ANCHORS.md#s27), 05.5에서 이동한 최종 native/Vault 진단 위치.

1. 최종 앱 bundle과 같은 source의 dylib를 빌드한다. 실제 사용 library 경로·signing/entitlements·native target을 기록한다. source path와 key material 원문은 로그에 노출하지 않는다.
2. 새 개발 profile에서 create→key insert→read back→DB open→identity/schema check→unlock을 실행한다. 최초 실패 stage와 safe OS code/incident를 수집한다. 정상 key 없음과 access denied를 혼동하지 않는다.
3. 실패 단계가 key lookup/insert인지, marker/schema/권한/host lock인지 구분한다. 성공했던 helper와 normal app의 차이는 그 단계에 필요한 범위만 비교한다. sandbox 때문이라고 미리 단정하지 않는다.
4. 원인에 맞게 platform/Vault adapter를 수정한다. 기존 키를 교체하거나 plaintext fallback·권한 확인 해제로 green을 만들지 않는다. schema reset이 필요한 경우에도 source/model credential과 외부 authority는 별개다.
5. 정상 앱에서 create/unlock→읽기→종료→재실행을 확인한다. helper 성공은 helper evidence로만 남긴다. 사용할 Apple SDK/Keychain 권한이 없으면 정확한 environment_blocked로 기록한다.

## 09.3 일반 앱에서 기존 기능과 장애 UX 검증

1. 실제 앱에서 `인사→답변 저장→두 번째 입력`을 수행하고 Session revision과 Run 결과가 일치하는지 확인한다. local 모델과 사전 승인된 remote profile은 각각 검증하며 자동 fallback으로 성공을 대체하지 않는다.
2. 응답 대기 중 Connect 목록/권한 preview를 열고, 창 이동·화면 재생성 뒤 같은 Run을 관측한다. 명시적 Stop은 CancelRun으로만 전달되어야 한다. transport timeout과 작업 실패를 UI에서 구별한다.
3. Day capture/edit, Knowledge 후보 검토, 승인 Action, OAuth operation 재진입을 canonical API에서 확인한다. destructive external write는 승인된 테스트 account/범위로 제한한다. permission 없는 실제 계정 데이터를 수정하지 않는다.
4. 승인/연결 mutation은 성공했는데 후속 projection이 실패하는 경우를 표시한다. 성공을 취소된 것으로 되돌리거나 동일 mutation을 새 ID로 재전송하지 않는다.
5. 앱 종료·재시작 후 SessionId만으로 기존 active/terminal Run과 operation을 찾는지 확인한다. Uncertain action은 조회·reconcile로 처리하고 자동 재실행하지 않는다.
6. iPhone/iPad는 필요한 SDK·기기가 있을 때 별도 실제 범위로 기록한다. macOS 성공을 iOS 또는 Android 성공으로 확대하지 않는다. Android parity는 여전히 범위 밖이다.

## 09.4 실제 LLM 위임 평가와 최종 종료

1. deterministic test는 선택한 AgentId가 바뀌지 않았는지 검증했다. 여기서는 실제 모델이 의미를 이해해 적절한 Expert를 고르는지 따로 평가한다.
2. 최소 입력군은 직접 인사, 일정 요청, 메일/관계 요청, 두 domain이 함께 필요한 요청, source 미연결 요청, Expert 실패 뒤 제한 설명이다. Calendar 설치 상태만 바꾸어 root/카드가 편향되는지 비교한다.
3. 성공 기준은 선택의 타당성, 부적절한 추가 위임 억제, 권한 없는 도구 실제 실행 0, 실패 사실을 감추지 않는 답변, 최종 응답 전달이다. 최초 성공 한 번을 안정성 비율로 바꾸지 않는다. 표본 수·모델/profile·실제 관측을 기록한다.
4. 구조 결함이 발견되면 영향받은 owner/계약만 수정하고 08 관련 검사를 재수행한다. 기존 runtime/전역 catch/하드코딩된 Expert 분기를 복원하지 않는다.
5. 원장에 구조 결과와 behavior 결과를 분리해 종료한다. 소스 snapshot, 실제 명령, 환경, 실패/미검증, 남은 정확한 작업을 남긴다. R003의 계획 본문을 실행 로그로 수정하지 않는다.

**전체 종료:** 지원된 현재 기능과 필수 안전·장애 흐름을 같은 단일 구현에서 확인했다. 미지원 환경은 따로 남긴다. 문서 발행, 많은 테스트 개수, 컴파일 성공만으로 제품 안정성 완료를 선언하지 않는다.
