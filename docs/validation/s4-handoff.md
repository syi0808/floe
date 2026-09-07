# S4 진행 현황 및 재개 체크리스트

기준일: 2026-09-07. 구현 기준 커밋: `e57c342`.
사용자 요청으로 Goal은 일시정지했다. 이 문서는 작업 인계용이며 S4 완료 선언이 아니다.
S4 수용 기준은 **0/14**, S1은 **0/4**, S3는 **2/5**다.
이는 최종 검증 완료 수이며 구현률이 아니다. 기준 정의는
[vertical slice delivery](../planning/08-engineering/vertical-slice-delivery.md)를 따른다.

## 완료되어 저장소에 포함된 구현

| 영역 | 구현된 범위 | 근거 및 제한 |
| --- | --- | --- |
| Agent runtime와 sample panel | 공통 versioned command/event, bounded multi-turn, stop/resume, 오류 및 중단 복구 | [Runtime](s4-agent-foundation.md), [panel](s4-agent-panel.md); 실제 connected personal chat과 구분 |
| 암호화 vault | Person별 암호화 세션, protected key 확인, key-unavailable 시 fail-closed, native worker의 명시적 setup/unlock | [Vault](s4-agent-vault.md), [native host](s4-agent-vault-host.md); 실제 앱 key lifecycle gate는 미완료 |
| Expert와 registry | package/version/install/assignment, enablement, Schedule/declarative 공통 계약, private state와 결과의 원자적 저장 | [Expert](s4-expert-foundation.md), [persistence](s4-expert-persistence.md), [management](s4-registry-management.md) |
| Calendar 권한과 View | 정확한 source scope의 불변 binding, 기본 비활성 atomic setup/receipt, UI 확인 및 별도 enablement, bounded Timeline projection | [Bindings](s4-calendar-bindings.md), [setup](s4-calendar-setup.md), [consent](s4-calendar-consent.md), [Timeline](s4-calendar-timeline.md) |
| Core Calendar turn | 저장된 권한과 lease 검증, 모델/Expert 실행, 결과 저장 및 Manager를 통한 S3 제안 준비 | [Core turn](s4-calendar-turn.md), [Manager](s4-manager-actions.md); Expert가 직접 외부 쓰기를 실행하지 않음 |
| 제안 조회와 검토 연결 | 저장된 제안의 기존 action을 읽기 전용으로 조회, 대화 카드 상태 표시, 기존 S3 검토 화면 연결 | [Inspection](s4-proposal-inspection.md), [presentation](s4-proposal-presentation.md); 조회는 재발행/승인/실행이 아님 |
| Calendar 전용 세션 | 암호화된 start/resume/get/recover, setup/provider scope 고정, sample 세션과 분리, native/Dart 저장 API | [Sessions](s4-calendar-sessions.md); Calendar 전용 실행 controller/UI는 아직 없음 |
| Local model adapter | 공통 계약의 bounded Foundation Models adapter와 availability probe | [Local model](s4-local-model.md); 실제 생성과 Personal 입력 검증은 미완료 |

최근 완료 커밋:

- `2e00e1c`: 저장된 proposal 상태 카드와 S3 검토 연결.
- `e57c342`: 격리된 암호화 Calendar 세션과 native recovery.

현재 일반 앱 경로는 암호화 sample chat이다. Calendar scope 설정과 빈 Personal 세션
저장이 가능하다는 사실만으로 실제 개인 데이터 대화가 준비되었다고 보지 않는다.

## 일시정지 시점의 로컬 미커밋 코드

다음 세 파일은 **이번 문서 커밋 및 push에 포함하지 않는다**.
다른 컴퓨터에서 이 문서 커밋을 checkout해도 이 작업 중 코드는 복원되지 않는다.
현재 작업 디렉터리에 보존하며, 다음 구현 작업에서 검증 후 별도로 커밋한다.

- `crates/floe-protocol/src/dto.rs`: Calendar turn 요청, 명시적 모델 선택,
  prompt 종류, destination 및 proposal outcome DTO 초안.
- `crates/floe-ffi/src/vault_host.rs`: owned worker의 Calendar turn dispatch와
  완료 응답 연결, 테스트용 runner factory 주입 경계.
- `crates/floe-ffi/src/vault_host/calendar_turn.rs`: 새 파일. 저장된 setup과 현재
  연결 revision에서 bounded lease를 구성하고 Core 실행 및 Manager 제안 준비를 기다리는 코드.

초안은 deterministic/Foundation Models를 명시적으로 구분한다. Production runner는
여전히 Synthetic만 허용하고 Personal 입력 및 silent remote fallback을 허용하지 않는다.
대화의 `Finished` event와 제안 준비까지 끝난 native job 완료를 구분하도록 작성했으나,
이 lifecycle 경계의 새 통합 테스트는 아직 없다. Dart turn transport/controller/UI도 없다.

## 검증 상태

### 완료 커밋 `e57c342`

- Workspace Rust 테스트 216개, keyring example 테스트 3개, native assertion 25개 통과.
- Flutter 테스트 189개, analysis 통과. 기존 UI golden 변경 없음.
- Rust formatting 및 기존 Calendar lint 제외를 적용한 Clippy 통과.
- Rust FFI 재빌드, macOS Debug 빌드, deep/strict codesign 검증 통과.
- 명령과 세부 근거는 [Calendar sessions 검증 기록](s4-calendar-sessions.md#evidence)에 있다.

### 로컬 미커밋 초안

- `CARGO_INCREMENTAL=0 cargo test -p floe-ffi --lib`: 컴파일 및 기존 테스트 18개 통과.
- 로컬 실행 로그: `/tmp/floe-s4-native-calendar-focused.log`.
  임시 로그는 저장소에 포함하지 않으며 장기 보존 증거가 아니다.
- 새 Calendar turn 전용 테스트, 전체 회귀, Flutter/native 앱 재빌드는 아직 하지 않았다.
  위 완료 커밋의 전체 검증 결과를 이 초안의 검증 결과로 확대 적용하지 않는다.
- 실제 protected key 생성 성공, 실제 model generation, 개인 source를 사용한 chat은
  이 테스트들로 검증하지 않았다. 문서 정리 시 테스트를 새로 실행한 것도 아니다.

## 재개 순서

1. **Native Calendar turn 초안 검토 및 통합 테스트.** 저장 scope/Person/revision,
   잘못된 day/window, stale/disconnected/revoked source, 잘못된 destination을 검증한다.
   Canonical grant bounds를 native model availability 확인 전에 검증하는 순서도 검토한다.
   Synthetic briefing/focus, destination 없는 결과, duplicate submit과 action 중복 방지를 확인한다.
2. **취소와 완료 경계 테스트.** model/provider 대기 중 stop/deadline/key loss,
   release/recover와 실행 중 job 충돌을 확인한다. 특히 `Finished` 이후 Manager 준비가
   끝나기 전에는 `done`이 되지 않아야 한다. 응답 유실은 저장된 action 조회로 대조하고
   모델이나 외부 쓰기를 맹목적으로 재실행하지 않는다.
3. **Dart transport와 Calendar controller/UI 연결.** sample 경로와 분리해 모델/실행을
   명시적으로 선택하고 streaming/stop/retry/resume/recovery를 연결한다. 응답의
   Person/session/setup/model과 proposal 결과를 검증하고 lock/late response를 처리한다.
   action 없음, 조회 전, 실패, 준비 완료를 구분하고 기존 S3 검토만 재사용한다.
4. **통합 회귀 후 별도 구현 커밋.** focused 테스트부터 workspace Rust, keyring example,
   native 검증, Flutter 전체/analyze, formatting/Clippy, FFI/macOS 빌드 및 서명 검증으로 확대한다.
5. **실제 key/model/source 및 수용 기준 검증.** 아래 환경 gate와 S1/S3 선행 검증을
   통과한 범위에서만 실제 개인 대화 경로를 연다. 이후 남은 S4 모델·connector를 진행한다.

## 실제 환경 gate와 S4 전체 잔여 범위

- **Protected key / P0-F:** 이미 `keyring-core`와 `apple-native-keyring-store`를 사용한다.
  앱이 Keychain API를 직접 구현할 필요는 없지만, 선택한 protected backend의 OS 권한
  경계가 사라지는 것은 아니다. 마지막 signed smoke는 create/cleanup에서 entitlement
  오류가 났다. matching provisioning/entitlements로 create/reopen/key-loss/cleanup을
  검증하고 실제 앱의 잠금·거부·crash·repair/deletion 경계를 확인해야 한다.
  불확실한 smoke root의 식별 파일은 정확한 slot cleanup 전 삭제하지 않는다.
  [재현 및 cleanup 절차](s4-keyring-live-smoke.md).
- **Local model / M1:** 마지막 availability 결과는 `AppleIntelligenceNotEnabled`다.
  이는 당시 관측이며 현재 설정을 재확인한 결과가 아니다. 사용 가능한 환경에서
  synthetic live generation부터 검증하고 budget/cancel/injection 평가를 수행해야 한다.
- **Remote/auth/privacy / M1–M3:** supported remote adapter의 공통 계약 연결과 live 검증,
  공식 third-party Codex 인증의 consent/refresh/revoke/mismatch 지원 가능성 확인이 남았다.
  지원 불가이면 ADR로 기록하고 supported 대안을 유지한다. 기존 credential 복사 없이
  모델 선택 전 정책, 전송 동의, outbound capture와 민감 raw data 차단을 검증해야 한다.
- **Agent / A1–A5:** 실제 connected multi-turn UI와 재시작 복구, layered context,
  live model 평가, trace/replay, 실패 격리 및 policy/executor 경계의 수용 증거가 필요하다.
  기존 자동 테스트와 구현 increment만으로 전체 criterion을 완료 처리하지 않는다.
- **Connectors / C1–C4:** 공통 Connections/View의 freshness/degraded 상태를 실제 Today
  briefing에서 소비해야 한다. Gmail OAuth/import/search/metadata/on-demand body와
  checkpoint/revoke/rate limit, Contacts limited/full/denied identity, location/MapKit
  ETA/WeatherKit leave-by 및 attribution의 실제 연결 검증이 남았다.
- **Physical Apple devices / C5–C6:** Screen Time public API/entitlement/region gate와
  coarse attention View 또는 검증된 unsupported ADR, iPhone/iPad HealthKit 최소 derived
  View 및 denial/no-data/stale/revocation 검증이 필요하다. private API 우회는 하지 않는다.
- **선행 조건:** S1의 controlled live Calendar 검증과 S3의 남은 승인/차단/실패/복구
  acceptance를 마쳐야 한다. Memory, voice, cross-device, background 실행은 이번 S4 범위가 아니다.

이 문서 커밋은 작업 인계만 수행한다. Goal 재개, 미검증 구현 승인, 실제 환경 설정 변경을
의미하지 않는다.
