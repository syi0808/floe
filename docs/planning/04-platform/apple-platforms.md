# Apple Platforms

> Status: Platform direction

## macOS

Floe의 초기 주력 플랫폼.

역할:

- full Day Canvas
- ambient wake word
- streaming voice
- desktop context
- transcription
- local model execution
- deep interaction

## iOS

항상 자체 wake word를 듣는 기능을 기본 전제로 하지 않는다.

핵심 빠른 invocation:

```text
Back Tap ×3
→ Floe App Intent / Shortcut
→ Voice Session
```

같은 intent를 시스템 surface에 재사용할 수 있는 구조를 선호한다.

## Health

HealthKit 원시 데이터는 local sensitive compute 경계에 둔다.

## Calendar / Reminder / Contacts

OS-native connector/provider로 구현하는 방향을 우선한다.

외부 SaaS connector abstraction과 OS provider abstraction은 구분한다.

## Apple Watch

JARVIS-style interaction의 중심은 아니다.

사용 가치가 명확할 때:

- health source
- notification
- glanceable state

위주로 지원한다.

## Apple-specific 기능과 Core

Apple API의 개념이 Floe Core domain schema를 지배하지 않도록 한다.

예:

HealthKit sample schema를 그대로 Personal State schema로 사용하지 않는다.
