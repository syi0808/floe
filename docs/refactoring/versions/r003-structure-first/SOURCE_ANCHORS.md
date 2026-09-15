# R003 고정 소스 앵커

기준: `cfde8e24387454d519c9e3308606a7cc6bb7f6c9`. 제품 소스는 `89452eb5523ef6b1c76b7fe857095748d76de22d`와 같은 tree다. 이 표는 읽기 시작 구간이며 자동 적용할 patch/AST 전체 범위가 아니다.

각 단계는 아래 ID를 인용한다. 숫자 줄은 원본 commit에서만 고정된다. 앞 단계가 파일을 옮기면 원장의 이동 위치와 심볼로 계속한다. 신규 파일의 미래 줄 번호는 만들지 않는다. 본문과 직접 호출자는 반드시 읽는다.

```bash
BASE=cfde8e24387454d519c9e3308606a7cc6bb7f6c9
git show "$BASE:path/to/file" | nl -ba | sed -n 'START,ENDp'
rg -n --fixed-strings -- "SYMBOL" path/to/current/file
git diff "$BASE" -- path/to/file
```

`source-anchors.json`은 읽기 위치·전체 blob SHA 목록이지 변경 상태 원장이 아니다. 같은 blob이면 원문을 재감사할 필요는 없지만 변경할 함수와 소비자는 읽는다. 범위가 EOF를 넘으면 EOF에서 멈추고, 심볼이 창 밖이면 같은 파일에서 검색해 정의 전체를 확인한다.

| ID | 읽을 파일·원본 줄 | 확인 심볼 | 전체 blob SHA |
|---|---|---|---|
| <a id="s01"></a>S01 | [Cargo.toml:L1–L3](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/Cargo.toml#L1-L3) | `workspace` | `05acc076ec6f047c1897f17feee083e3fad6a1e1` |
| <a id="s02"></a>S02 | [crates/floe-protocol/src/dto/mod.rs:L1–L12](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-protocol/src/dto/mod.rs#L1-L12) | `APP_WIRE_VERSION` | `139aea0b7db88b192abac3ee3dcf2a1d93894e74` |
| <a id="s03"></a>S03 | [crates/contracts/agent/src/ports.rs:L1–L80](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/contracts/agent/src/ports.rs#L1-L80) | `JournalEvent` | `c79dd638b8dcdcaa9008075a424078320331ca25` |
| <a id="s04"></a>S04 | [crates/modules/context/src/ports/archive_reader.rs:L1–L74](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/context/src/ports/archive_reader.rs#L1-L74) | `ArchiveReadRequest` | `32fe5cb382992d3a34e19a13db8c33e8e974d899` |
| <a id="s05"></a>S05 | [crates/modules/conversation/src/ports/mod.rs:L1–L85](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/conversation/src/ports/mod.rs#L1-L85) | `SessionArchiveRepository` | `2f93c1d01ad490f1c12308790f34b80a9948ff8d` |
| <a id="s06"></a>S06 | [crates/modules/conversation/src/api.rs:L1–L120](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/conversation/src/api.rs#L1-L120) | `TurnRequest` | `35a622ffd753cefc8d5429576e29ed31b38ba260` |
| <a id="s07"></a>S07 | [crates/modules/conversation/src/application/coordinator.rs:L1–L220](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/conversation/src/application/coordinator.rs#L1-L220) | `run_turn_observed` | `3cd26dabf620a5115a268ea036b2b4a3a3e0a3b1` |
| <a id="s08"></a>S08 | [crates/floe-core/src/agent_vault/conversations.rs:L1–L165](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-core/src/agent_vault/conversations.rs#L1-L165) | `exact_admission` | `f733deea847f2337a425fe48997937cd9276d3eb` |
| <a id="s09"></a>S09 | [crates/floe-ffi/src/vault_host/conversation_turn.rs:L360–L490](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/vault_host/conversation_turn.rs#L360-L490) | `request_context` | `74bb16ab647133409a4d9ddb3fa4e7d2c5953039` |
| <a id="s10"></a>S10 | [crates/floe-ffi/src/lib.rs:L35–L180](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/lib.rs#L35-L180) | `LegacyComposition` | `3d4db76cae6663820e670aef12d3119b4da6b4fe` |
| <a id="s11"></a>S11 | [crates/floe-ffi/src/inference_routes.rs:L1–L126](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/inference_routes.rs#L1-L126) | `HostInferenceRoutes` | `3f6b6d66df75d5dc3801f1d00d34864455f05a1b` |
| <a id="s12"></a>S12 | [crates/floe-infra/src/remote_model.rs:L185–L345](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-infra/src/remote_model.rs#L185-L345) | `resolve_remote_model_route` | `bbcdabb5429c57b0e7de3df0bbada3f7293b6710` |
| <a id="s13"></a>S13 | [crates/modules/connections/src/application/pairing.rs:L1–L118](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/connections/src/application/pairing.rs#L1-L118) | `PairingService` | `779a746fbf8710d0e196f842d892fb4c635c3d3a` |
| <a id="s14"></a>S14 | [apps/client/lib/features/day_canvas/presentation/connector_screen.dart:L130–L240](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/apps/client/lib/features/day_canvas/presentation/connector_screen.dart#L130-L240) | `_loadCatalog` | `ff13c33d3894b8c296cf1e49a5f571d8135a0925` |
| <a id="s15"></a>S15 | [apps/client/lib/features/day_canvas/presentation/server_connector_panel.dart:L45–L250](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/apps/client/lib/features/day_canvas/presentation/server_connector_panel.dart#L45-L250) | `_waitForPoll` | `c94a666f7634711aa578daa245bac6386b6670e8` |
| <a id="s16"></a>S16 | [crates/floe-core/src/calendar_action.rs:L1–L165](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-core/src/calendar_action.rs#L1-L165) | `CalendarActionProvider` | `1fdeaca80f57269c955f1bb8bf4a0f88bba9eed6` |
| <a id="s17"></a>S17 | [crates/floe-core/src/calendar_action.rs:L290–L515](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-core/src/calendar_action.rs#L290-L515) | `execute_calendar_action` | `1fdeaca80f57269c955f1bb8bf4a0f88bba9eed6` |
| <a id="s18"></a>S18 | [crates/floe-core/src/lib.rs:L1–L60](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-core/src/lib.rs#L1-L60) | `ActionAuthority` | `d2b8a2998451a8e4c9cae24a60f41c4c889ea263` |
| <a id="s19"></a>S19 | [crates/modules/day/src/lib.rs:L1–L15](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/day/src/lib.rs#L1-L15) | `DayService` | `e826eb60951585b37e4b1870606451d819335322` |
| <a id="s20"></a>S20 | [crates/modules/knowledge/src/lib.rs:L1–L58](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/knowledge/src/lib.rs#L1-L58) | `pub mod application` | `2c933786f72077e00537652551063d027b731921` |
| <a id="s21"></a>S21 | [crates/modules/knowledge/src/ports/repository.rs:L1–L33](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/knowledge/src/ports/repository.rs#L1-L33) | `LearnerJobRepository` | `be3f22fe7dc287dbc8ccd05c81f5367804577d1e` |
| <a id="s22"></a>S22 | [server/internal/console/console.go:L1–L240](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/server/internal/console/console.go#L1-L240) | `type Console struct` | `8d5756d7a1326dae744cfacdb5ec2a31844c6593` |
| <a id="s23"></a>S23 | [crates/runtime/agent/src/engine.rs:L90–L245](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/runtime/agent/src/engine.rs#L90-L245) | `record_intent` | `5590568ae10ea71749189d299cf417854f5a7457` |
| <a id="s24"></a>S24 | [crates/runtime/agent/src/engine.rs:L245–L475](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/runtime/agent/src/engine.rs#L245-L475) | `ModelStep::Delegate` | `5590568ae10ea71749189d299cf417854f5a7457` |
| <a id="s25"></a>S25 | [crates/modules/inference/src/application/attempt.rs:L1–L85](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/inference/src/application/attempt.rs#L1-L85) | `AttemptLifecycle` | `5e8b2b82e1c79895ffde759cd22a789ba80da33b` |
| <a id="s26"></a>S26 | [crates/modules/experts/src/task.rs:L1–L200](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/modules/experts/src/task.rs#L1-L200) | `TaskRepository` | `fcaf4256c2a39b062ebf30a1312639f51ab66c7c` |
| <a id="s27"></a>S27 | [crates/floe-ffi/src/vault_host.rs:L1–L325](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/vault_host.rs#L1-L325) | `struct Worker` | `eeb8a956e20785a8a2960982650240a37c9affdb` |
| <a id="s28"></a>S28 | [crates/floe-ffi/src/vault_host/conversation_repository.rs:L1–L260](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/vault_host/conversation_repository.rs#L1-L260) | `VaultConversationRepository` | `1c6d940c5573752b2f1fac7d0a79525e95917af1` |
| <a id="s29"></a>S29 | [crates/floe-core/src/agent_vault/keyring.rs:L1–L87](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-core/src/agent_vault/keyring.rs#L1-L87) | `KeyringVaultKeys` | `04f1eef4847aa9c9748c69e4430faa7bdc7fc1f8` |
| <a id="s30"></a>S30 | [crates/app/src/host.rs:L1–L158](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/app/src/host.rs#L1-L158) | `legacy_services` | `443c5be1cd52d4a8506026d22e2364a9a7b0d46e` |
| <a id="s31"></a>S31 | [crates/floe-ffi/src/abi.rs:L1–L180](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/abi.rs#L1-L180) | `invoke_json_v2` | `9cd88a8238069643cb5a01c58af3a4dc26aa2fbe` |
| <a id="s32"></a>S32 | [crates/floe-ffi/src/app_wire.rs:L1–L190](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/app_wire.rs#L1-L190) | `command_with_host` | `8a3912a8aa80ae05b4b5d3dd0ae548d18b19a414` |
| <a id="s33"></a>S33 | [apps/client/lib/features/day_canvas/application/ffi_day_gateway.dart:L1–L95](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/apps/client/lib/features/day_canvas/application/ffi_day_gateway.dart#L1-L95) | `_runtimeClient` | `e26c0265e4d33574b7959c5b8b5acdb74946010e` |
| <a id="s34"></a>S34 | [apps/client/lib/runtime_client/read_model/app_read_model.dart:L1–L180](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/apps/client/lib/runtime_client/read_model/app_read_model.dart#L1-L180) | `canSend` | `a44309602b8e66753021204c6465b5a77a199eaf` |
| <a id="s35"></a>S35 | [apps/client/macos/build_rust.sh:L1–L40](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/apps/client/macos/build_rust.sh#L1-L40) | `--package floe-ffi` | `937e46454899498f6ed075dcf2fc9317af8e9ace` |
| <a id="s36"></a>S36 | [tools/architecture/check_boundaries.py:L1–L140](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/tools/architecture/check_boundaries.py#L1-L140) | `production_deps` | `6adf7b3ac3e5097908b4290f39ab67de97a0f050` |
| <a id="s37"></a>S37 | [crates/floe-ffi/src/vault_host/conversation_turn/expert_dispatch.rs:L1–L180](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/vault_host/conversation_turn/expert_dispatch.rs#L1-L180) | `LegacyExpertEndpoint` | `01f9329f86c277924945105fbc8d64e289f8c9ac` |
| <a id="s38"></a>S38 | [crates/floe-ffi/src/vault_host/conversation_turn/expert_dispatch.rs:L280–L530](https://github.com/syi0808/floe/blob/cfde8e24387454d519c9e3308606a7cc6bb7f6c9/crates/floe-ffi/src/vault_host/conversation_turn/expert_dispatch.rs#L280-L530) | `agent_cards` | `01f9329f86c277924945105fbc8d64e289f8c9ac` |

## 추가 직접 소비자

위 창만 읽고 기계적으로 삭제하지 않는다. 해당 이름의 실제 호출자는 `rg -n`으로 찾는다. 아래 파일들은 같은 고정 commit의 관련 구현이며 각 단계에 검색할 심볼을 명시했다. 전체 파일에 대한 새 전수 감사 결과로 제시하는 목록은 아니다.

- `crates/modules/conversation/src/application/{coordinator,finalization,recovery,session,archive}.rs`
- `crates/floe-ffi/src/vault_host/{task_repository,conversation_repository,conversation_turn}.rs`
- `crates/floe-core/src/agent_vault/{conversations,tasks}.rs` 및 actual module declarations가 지시하는 Access/Knowledge 저장 구현
- `apps/client/lib/features/agent/{agent_controller,agent_vault_gateway}.dart`
- `apps/client/lib/features/conversation/conversation_runtime_gateway.dart`
- `apps/client/lib/runtime_client/floe_client.dart`
- `apps/client/lib/infrastructure/native/{native_transport,native_bindings}.dart` — binding의 실제 파일명은 `FloeNativeBindings` 정의 검색으로 확인
- `server/internal/console/trust_store.go`, `server/cmd/floe-server/main.go`
- `apps/client/ios/build_rust.sh`, Apple Xcode project build phases 및 Swift native callbacks

앵커는 저장소 커넥터의 고정 SHA 읽기와 직전 동일 product tree 읽기를 사용했다. 로컬 전체 Git checkout을 내려받아 앵커 검사를 실행한 것은 아니다. 실행자는 시작 시 `git show`로 대조한다. 문서 작성 환경에는 GitHub 직접 DNS 접근과 Cargo/Flutter가 없어 제품 빌드·실행을 수행하지 않았다.
