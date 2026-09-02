# People & Relationships

> Status: Core domain

## 목적

Floe가 사용자의 개인 비서라면 사람 관계는 1급 데이터여야 한다.

대상:

- 연인
- 가족
- 친구
- 동료
- 기타 중요한 사람

## Person

개념적 구조:

```text
Person
├─ identity
├─ aliases
├─ preferences
├─ important dates
├─ recent context
├─ commitments
└─ episodes
```

## Identity Resolution

동일 인물이 여러 identity로 나타날 수 있다.

```text
민수
김민수
민수형
phone number
email
calendar attendee
contact identifier
```

잘못 merge하면 개인 비서로서 영향이 크므로 uncertain link를 바로 확정하지 않는다.

## Relationship

관계는 시간에 따라 변한다.

```text
Relationship {
  person
  type
  validFrom
  validUntil
  context?
}
```

예:

"현재 여자친구" 같은 관계를 immutable property로 보지 않는다.

## 관계 추론

"요즘 걔랑 좀 어색해" 같은 발화를:

```text
relationship = bad
```

로 저장하지 않는다.

대신 관찰/추론/시점/confidence를 유지한다.

## Future: Shared Context

부모/자녀 등 서로 다른 Floe Person 사이에서 명시적으로 공유된 context를 지원할 수 있다.

원칙:

- private by default
- explicit sharing
- scoped
- revocable
