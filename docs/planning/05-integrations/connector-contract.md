# Floe Connector Contract

> Status: Draft interface

## 목적

Floe가 integration abstraction을 직접 소유한다.

Activepieces/n8n/기타 source의 API를 Floe Core에 직접 노출하지 않는다.

## 개념적 Contract

```ts
interface FloeConnector {
  manifest: ConnectorManifest

  auth: {
    connect(): Promise<Connection>
    refresh(): Promise<void>
    revoke(): Promise<void>
  }

  capabilities(): Capability[]

  sync?: {
    bootstrap(): AsyncIterable<Entity>
    changes(cursor: Cursor): AsyncIterable<Change>
  }

  subscribe?: (
    emit: (event: ConnectorEvent) => void
  ) => Subscription

  execute(
    action: ConnectorAction
  ): Promise<ActionResult>
}
```

실제 언어/형식은 미정이며 위 코드는 semantic sketch다.

## Observe, Act and Interact

Connector manifest의 capability는 최소한 다음 authority class를 구분한다.

```text
Observe
  sync/change/read → bounded provider-neutral View

Act
  typed proposal → Policy/Review/Validation/Executor → execute

Interact provider
  invoke/listen/report/notify/show
```

Observe와 Act grant는 별도다. Observe 결과는 source, scope, freshness, retention,
sensitivity와 provenance를 포함하며 credential이나 provider-native object를 Agent/Expert에
노출하지 않는다. Act는 read grant에서 추론할 수 없고 항상 governed action boundary를
통과한다.

Voice, notification, lock screen, watch, car와 visual UI는 사용자를 호출하고 결과를
전달하는 Interact provider다. Connections 화면에 함께 나타날 수 있지만 data connector와
동일한 credential, lifecycle 또는 execution authority를 갖는다고 가정하지 않는다.

## Capability-driven

모든 connector가 동일 기능을 지원한다고 가정하지 않는다.

예:

```text
Gmail
readMessages    ✓
subscribe       ✓
createDraft     ✓
send            ✓
```

Manager/Skill은 capability를 확인한 뒤 행동해야 한다.

## Sync

Floe에서는 일반 automation connector보다 다음이 더 중요할 수 있다.

- bootstrap
- incremental change
- update/delete
- revision
- cursor
- provenance
- conflict

## ConnectorConnection

외부 서비스 계정 연결은 Floe Account와 구분한다.

Person ownership과 execution binding은 ADR 0025를 따른다. 제안된 공통 authority 설계에서
connection은 실제 source 접근, `DataAccessGrant`는 AI 사용 동의, `ContextObservation`은
관측된 evidence를 표현한다. 연결됨을 AI 사용 동의로 해석하지 않는다.

개념적으로 (목표 모델이며 기존 wire DTO가 아님):

```text
ConnectorConnection {
  personId
  connectionId
  connectorId
  sourceIdentityRef
  executionOwner
  credentialRef?
  selectedSourceScope
  sourceEpoch
}
```

정상 sync와 token refresh는 source/grant authority epoch를 증가시키지 않는다. immutable
observation ID, storage CAS version, provider cursor/revision은 별도로 다룬다. 모든 connector에
같은 snapshot 저장 방식을 강제하지 않는다.

scope 교집합, 철회, 기능별 recovery와 단계적 전환은
[Connection, Access & Observation](connection-access-and-observation.md) 및
[runtime 설계](../09-implementation/connection-authorization-runtime.md)를 따른다.

## Security

Credential raw value를 domain object에 포함하지 않는다.

별도 credential vault/reference를 사용한다.

## Open Questions

- connector process isolation
- TS connector와 native connector의 protocol
- remote connector worker
- webhook ingress
- version compatibility
