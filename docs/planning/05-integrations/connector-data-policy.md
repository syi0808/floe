# Connector Data Policy

> Status: Recommended baseline

## Principle

Connector마다 동일한 storage/sync policy를 적용하지 않는다.

외부 시스템의 원시 데이터를 Floe 전체 DB에 무조건 복제하는 것은 피한다.

```text
Source
  ↓
Connector
  ↓
Domain-specific retention policy
  ↓
Mirror / Projection / Derived State / No Retention
```

## Policy Classes

### Mirror

외부 object를 Floe가 일정 기간 normalized mirror로 유지한다.

예: Calendar Event.

### Index + On-demand

검색과 context에 필요한 metadata를 저장하고 큰/sensitive body는 필요할 때 원본 source에서 가져온다.

예: Email.

### Identity Reference

외부 record를 그대로 memory화하지 않고 Person resolution용 identity evidence만 보관한다.

예: Contacts.

### Derived Only

Raw source는 device/provider에 남기고 Floe는 derived state만 sync한다.

예: Health.

## Initial Mapping

| Domain | Policy |
|---|---|
| Calendar | Mirror |
| Mail | Index + On-demand |
| Contacts | Identity Reference |
| Health | Derived Only |
| Floe Task | Floe Canonical |
| Floe Note | Floe Canonical |

## Provenance

어떤 policy에서도 derived object가 원본 source와의 연결을 잃지 않도록 한다.

예:

```text
Commitment
  source:
    connectorConnectionId
    messageId
    extractionVersion
```

## External Content Safety

Mail body, calendar description, external contact notes 등은 untrusted content다.

Connector data가:

- system prompt
- model instruction
- persistent policy

로 승격되어서는 안 된다.
