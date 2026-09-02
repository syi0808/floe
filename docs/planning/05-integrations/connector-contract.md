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

개념적으로:

```text
ConnectorConnection {
  personId
  connectorId
  credentialRef
}
```

## Security

Credential raw value를 domain object에 포함하지 않는다.

별도 credential vault/reference를 사용한다.

## Open Questions

- connector process isolation
- TS connector와 native connector의 protocol
- remote connector worker
- webhook ingress
- version compatibility
