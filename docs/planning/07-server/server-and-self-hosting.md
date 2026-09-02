# Server & Self-hosting

> Status: Core distribution direction

## 목표

Floe는 오픈소스로 공개하고 서버 역시 self-host 가능하게 한다.

Hosted Floe는 OSS stack의 managed distribution에 가깝다.

## Deployment Modes

### Floe Cloud

일반 사용자/부모님에게 기본.

```text
Install
↓
Sign in
↓
Connect
↓
Use
```

### Self-hosted Floe

고급 사용자.

목표 설치 경험:

```bash
docker compose up -d
```

수준.

후보 구성:

```text
Floe API
Database
Memory
Manager Runtime
Connector Workers
Sync
Admin Dashboard
```

### Personal/Home Node

장기적으로:

- Mac mini
- home server
- NAS
- old PC

등을 Personal Floe Node로 사용하는 모델을 고려한다.

## Self-host Admin Dashboard

후보 기능:

- Account 생성/비활성화
- Person 생성
- Account ↔ Person Membership
- invitation
- connector health
- server health
- OAuth configuration
- provider/model configuration

## 기본 UX와 분리

일반 사용자가:

- Redis
- DB URL
- OAuth callback
- model endpoint

같은 설정을 보지 않도록 한다.

원칙:

> **Self-hostable by architecture, invisible by default.**

## OAuth

초기 self-host는 BYO OAuth credentials로 시작할 수 있다.

장기적으로 선택적인 Floe-managed OAuth broker를 제공할 수 있다.

```text
Self-host
├─ Floe-managed OAuth
└─ Bring Your Own Credentials
```

Managed OAuth가 self-host의 필수 dependency가 되어서는 안 된다.
