# Floe Progress

> Last updated: 2026-09-11
>
> Purpose: 현재 진행 현황, acceptance 상태, 다음 검증만 한눈에 추적한다.
> 완료된 구현 increment와 과거 checkpoint는 [`docs/history/`](docs/history/README.md),
> 상세 근거는 [`docs/validation/`](docs/validation/)에서 관리한다.

## Current Focus

**S5.5 — Connected domains**의 production 경로를 acceptance evidence로 전환하는 단계다.

- Apple/Android Personal View와 Schedule 및 7개 확장 Expert의 durable, source-gated 경로가 통합됐다.
- device presence/scope, freshness, transfer class, disagreement 정책과 strict local FFI View store가 구현됐다.
- signed physical-device, live SaaS provider, 통합 failure/privacy corpus 근거가 아직 없다.
- 따라서 S5.5의 14개 기준은 모두 Pending이며 formal acceptance는 **0/14**다.
- 현재 기준의 세부 상태는 [S5.5 canonical status matrix](docs/validation/s5-5-status-matrix.md)가 유일한 source of truth다.
- local FFI store는 cross-device sync가 아니다. Device Gateway, lease, E2E relay와 two-device convergence는 S8 범위다.

## Delivery Board

Delivery는 [ADR 0006](docs/decisions/0006-slice-driven-delivery.md)과
[slice acceptance plan](docs/planning/08-engineering/vertical-slice-delivery.md)을 따른다.
Acceptance 수치는 구현량이 아니라 검증 완료 criterion 수다.

| Slice | Status | Acceptance | Primary blocker | Next demo |
| --- | --- | --- | --- | --- |
| S1 — Calendar read | Implementing | 0/4 | Controlled permission/DST/recurrence/lifecycle gates | Finish controlled live matrix |
| S3 — Approved action | Integrated; validating | 2/5 | S1 Verified; rejection/blocking/failure matrix; dogfood | Complete remaining acceptance matrix |
| S4 — Connected Agent/Experts | Implementing | 0/14 | S3 Accepted; live key/model/source/privacy gates | Validate a live on-device Calendar conversation |
| S5 — Memory/self-improvement | Planned; foundations started | 0/6 | S4 Accepted; P0-D corpus; P0-F local vault/key | Review, reuse and roll back one Memory and Playbook change |
| S5.5 — Connected domains | Implementing; reconciled | 0/14 | S5 Accepted; signed device/provider evidence; failure/privacy corpus | Complete one signed native-host personal-context scenario |
| S6 — Transcription/voice | Planned | 0/5 | S5.5 Accepted; streaming/recording STT/TTS PoC | Continue Agent chat by voice |
| S7 — Local wake-up | Planned | 0/4 | S6 Accepted; resident wake lifecycle | Wake phrase opens a visible local voice session |
| S8 — Cross-device/server | Planned | 0/4 | S7 Accepted; sync/security PoCs | Produce the same result on two devices |
| S9 — Intervention | Planned | 0/4 | S8 Accepted; intervention policy | Trigger a controlled suggestion from a Calendar change |

## Evidence Queue

1. Complete the S1 controlled live Calendar matrix.
2. Close the remaining signed-app S3 rejection, blocking, failure and dogfood gates.
3. Validate S4 with live key, local model, Calendar source and privacy evidence.
4. Exercise S5 Memory and Playbook review, reuse, rollback and corpus gates.
5. Run the S5.5 signed native-host personal-context scenario.
6. Run live Google/Microsoft/Android, Work Context and Life Logistics cohorts.
7. Capture the integrated S5.5 failure, recovery, disagreement and outbound privacy corpus.

## Known Boundaries

- Fixture, conformance and build checks do not satisfy a live acceptance gate.
- Screen Time stays typed unavailable until the public entitlement/authorization/region gate is evidenced; the macOS Attention heuristic is a separate source.
- Cross-device transport and convergence remain S8 work and cannot substitute for native-host S5.5 evidence.
- Deferred Personal Day breadth includes Event/Task/Note editing, general conflict recovery, dense-day folding and two-week dogfood; pull it forward only when an active slice requires it.

## History

- [2026-09-11 — Connected-domain expansion](docs/history/2026-09-11-connected-domains.md)
- [2026-09-10 — Agent, Memory and connector foundations](docs/history/2026-09-10-agent-memory-connectors.md)
- [2026-09-06–09 — Approved actions and connected Agent](docs/history/2026-09-06-to-09-actions-and-agent.md)
- [2026-09-03–05 — Personal Day and Calendar foundations](docs/history/2026-09-03-to-05-personal-day-calendar.md)

## Update Rules

- Update this file only when the current focus, acceptance count, blocker or next evidence changes.
- Move completed increment narratives to a dated file under `docs/history/` in the same commit.
- Keep immutable test runs and detailed evidence under `docs/validation/`; link rather than duplicate them.
- Do not duplicate product requirements or architecture decisions here.
- Advance status only with evidence, and distinguish fixture, sandbox and live evidence.
- Track acceptance counts against the slice plan; do not replace them with subjective percentages.
