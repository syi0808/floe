# Voice & Presence

> Status: Accepted direction, platform details evolving

## 목표

Voice는 채팅 입력의 음성 버전이 아니라 Floe의 **Presence Layer**다.

세 가지 사용 시나리오:

1. Invocation — Floe를 부른다.
2. Capture — 생각이나 요청을 빠르게 말한다.
3. Transcription — 회의/대화를 기록하고 구조화한다.

## macOS

macOS를 초기 ambient voice의 대표 플랫폼으로 삼는다.

장기 목표:

```text
"Floe"
   ↓
Local wake word
   ↓
Speaker recognition
   ↓
Streaming STT
   ↓
Manager
```

wake detection과 voiceprint 처리는 가능한 한 로컬에서 수행한다.

## iOS

항상 hotword를 듣는 구조는 핵심 전제로 삼지 않는다.

우선적인 빠른 호출 UX:

```text
iPhone Back Tap ×3
       ↓
Floe App Intent / Shortcut
       ↓
Voice Session
```

같은 intent를 향후 Action Button, Siri, Shortcut 등 다른 system surface에도 재사용할 수 있다.

## Android

Android는 assistant-level integration을 적극 활용할 수 있는 플랫폼으로 본다.

후보 surface:

- Assistant role
- Voice interaction
- Quick Settings
- shortcut
- notification interaction
- 제조사별 gesture

## Windows

macOS와 함께 ambient desktop Floe의 핵심 플랫폼이 될 수 있다.

- wake word
- global hotkey
- desktop context
- meeting transcription
- local model runtime

## Apple Watch

JARVIS-style voice UX의 핵심 플랫폼은 아니다.

지원한다면:

- health data source
- notifications
- glanceable state

위주로 본다.

## Speaker Recognition

Speaker recognition은 UX identity다.

```text
speaker match
→ likely user
```

민감한 행동 승인은 OS biometric/security 기능을 사용한다.
