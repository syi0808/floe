# Connector Sources & Reuse Strategy

> Status: Accepted direction

## 목표

Floe가 수십~수백 SaaS connector를 처음부터 직접 구현하지 않는다.

대신 검증된 connector ecosystem의 implementation을 재사용한다.

## Activepieces

현재 가장 관심 있는 source.

관심 있는 것은 workflow engine이 아니라:

- OAuth/authentication 구현
- API client
- actions/triggers
- 다양한 서비스 connector package

이다.

개념:

```text
Activepieces Piece
      ↓
Floe Adapter
      ↓
Floe Connector Contract
```

## n8n

같은 이유로 connector source는 유용할 수 있지만 전체 workflow runtime을 Floe architecture로 채택할 의도는 없다.

라이선스/embedding 제약이 있으므로 code reuse는 별도 검토가 필요하다.

## Vendor-neutral

```text
Floe Connector Contract
├─ Floe Native
├─ Activepieces Adapter
├─ Other OSS Adapter
└─ External Runtime Adapter
```

## 중요 원칙

외부 connector ecosystem을 교체하더라도 다음이 바뀌지 않아야 한다.

- Timeline
- Memory
- Manager
- Expert
- Skill semantics
- Connection UX

## 향후 가능성

반복 가능한 mapping이 충분하다면:

- connector adapter SDK
- source-to-Floe adapter generator
- 독립 `floe-connectors` repository

등을 검토할 수 있다.
