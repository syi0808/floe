# Privacy & Data Classification

> Status: Core product requirement

## 위협 수준

Floe는 잠재적으로 다음 데이터를 한 시스템 안에서 다룬다.

```text
Email
Calendar
Voice
Health
Location
Relationships
Personal History
Tasks
Notes
```

따라서 일반 생산성 SaaS보다 breach impact가 크다.

## Data Classification

초기 개념:

### Device-only

가능하면 서버에 보내지 않는다.

- voiceprint
- wake-word audio
- raw Health
- 일부 local credentials

### Highly Sensitive

- 연애/가족 갈등
- 세부 건강 개인사
- 민감 episode
- 재정 등 강한 개인 정보

### Personal

- Timeline
- Task
- 일반 preference
- 일반 Personal Memory

### Temporary AI Context

- remote reasoning을 위한 최소 projection
- 가능한 한 ephemeral

## 중요한 주의

"정제된 데이터"가 자동으로 비민감한 것은 아니다.

예:

```text
recovery = low
recent_interpersonal_stress = high
```

도 민감한 개인 정보일 수 있다.

## User Control

사용자는 connector/데이터별로 최소한 다음을 알 수 있어야 한다.

- 어떤 데이터에 접근하는가
- 어디에 저장하는가
- 외부 모델에 무엇이 전달되는가
- 어떤 행동 권한이 있는가

## Memory Access

Expert별 least-privilege memory view를 지향한다.

## Authentication

speaker recognition과 실제 인증을 분리한다.

민감 행동은 OS-level auth나 명시적 confirmation을 사용한다.

## Deletion

Memory delete가 derived artifact까지 추적할 수 있도록 provenance가 중요하다.

## Third-party Expert Boundary

Marketplace/User-installed Experts are independent principals for authorization purposes.

A Person granting Floe access to Gmail or Health does **not** imply every installed Expert can access that data.

```text
Person permission to Floe
        ≠
Expert permission to Person data
```

Expert-specific grants are stored on `ExpertAssignment`.

