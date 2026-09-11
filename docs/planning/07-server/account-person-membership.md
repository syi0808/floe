# Account, Person & Membership

> Status: Accepted foundational model

## 핵심 원칙

**Floe Account와 Floe Person을 분리한다.**

이는 초기에 잡지 않으면 Memory ownership, Health ownership, connector ownership, encryption key ownership을 나중에 크게 변경하게 만들 수 있다.

## Account

인증 주체.

```text
Account {
  id
  auth identities
  instance permissions
  status
}
```

Account는 "로그인할 수 있는 principal"이다.

## Person

Floe가 실제로 보좌하는 사람.

```text
Person {
  id
  profile
  timeline
  state
  memoryVault
  preferences
}
```

Personal Memory, Timeline, Health-derived state의 ownership은 Person 중심이다.

## Membership

```text
Membership {
  accountId
  personId
  role
  permissions
}
```

초기에는 대부분:

```text
Account A
  │ owner
  ▼
Person A
```

이지만 1:1을 강제하지 않는다.

## 부모님 관리 시나리오

```text
Account: Mother
   │ owner
   ▼
Person: Mother

Account: Yein
   │ manager
   └────────→ Person: Mother
```

manager가 곧 Personal Memory를 읽을 수 있는 것은 아니다.

운영 권한과 개인 데이터 권한을 분리한다.

예:

```text
Manage device       ✓
Manage connectors   ✓
Billing             ✓

Read memory         ✕
Read health         ✕
Read emails         ✕
```

## Cross-Person Context Sharing

Membership는 운영 관계이고 context read grant가 아니다. 다른 Person의 Agent가 일정,
상태 또는 coordination 정보를 필요로 하면 owner Person이 purpose-bound
`SharedContextGrant`를 별도로 발급한다.

- 일정은 busy/free와 timezone 같은 최소 availability projection만 공유한다.
- mood/social availability는 owner가 명시적으로 선언한 coarse short-lived state만 공유한다.
- inferred mood, raw Health, private Memory, mail body와 connector capability는 공유하지 않는다.
- offline 사용은 grant가 허용한 최신 encrypted snapshot과 expiry 안에서만 가능하다.
- revoke는 신규 query를 즉시 막고 저장된 projection에 tombstone을 전파한다.

전체 query/relay/action 흐름은
[ADR 0024](../../decisions/0024-device-context-collection-and-convergence.md)를 따른다.

## Person Without Login

Self-host admin이 Person을 먼저 만들고 login identity는 나중에 연결할 수 있는 여지를 둔다.

## Auth Identity

장기적으로 Account 아래 여러 auth identity를 둘 수 있다.

- passkey
- password
- Google
- Apple
- OIDC
- 기타

## Connector Account는 별개

Gmail/Outlook/GitHub connection은 Floe Account가 아니라 `ConnectorConnection`으로 다룬다.
