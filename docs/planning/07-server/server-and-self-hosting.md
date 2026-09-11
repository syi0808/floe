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
Device Directory / Gateway
Opaque Context Relay
Admin Dashboard
```

Go server는 cross-device control plane이지만 raw device-context warehouse가 아니다. Device
Gateway는 authenticated capability/presence metadata와 bounded ContextQuery lease를 중계한다.
Raw Health, raw Screen Time/activity와 precise location은 적재하지 않으며, 허용된 Highly
Sensitive projection은 기본적으로 server가 해독하지 않는 end-to-end encrypted payload로
relay한다. 상세 경계는
[ADR 0024](../../decisions/0024-device-context-collection-and-convergence.md)를 따른다.

### Personal/Home Node

장기적으로:

- Mac mini
- home server
- NAS
- old PC

등을 Personal Floe Node로 사용하는 모델을 고려한다.

## Self-host Admin Dashboard

초기 로컬 구현에서는 [ADR 0010](../../decisions/0010-local-connection-console.md)에 따라
로컬 단일 운영자용 연결 콘솔만 먼저 구현한다. 모델 target/API key 관리,
Codex OAuth 및 제한된 inference, 앱 페어링과 합성 데이터 연결 테스트가 범위다.
아래 계정·Membership·초대 기능이나 원격 서버 배포를 완료한 것은 아니다.

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

Floe Cloud는 Floe가 등록한 OAuth application, 고정 HTTPS callback과 server-side secret
storage를 제공하며 grant/token을 Person별로 관리한다.

Self-host는 자체 공개 URL에 맞는 OAuth application을 provider별로 등록하고 credentials를
설정하는 BYO 방식으로 시작한다. 임의의 self-host callback URL은 Floe의 공용 provider
registration으로 커버하거나 중앙 relay로 우회하지 않는다.

```text
Floe Cloud ── Floe-managed OAuth registration
Self-host  ── Bring Your Own OAuth registration
```

Client ID는 공개 식별자다. Client secret과 refresh token만 repository/client build에서
제외하고 server secret store에 보관한다.
