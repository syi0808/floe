# Voice & Presence

> Status: Accepted direction, platform details evolving

## 목표

Voice는 채팅 입력의 음성 버전이 아니라 Floe의 장기적인 **기본 interaction과
Presence Layer**다. Floe의 본체는 assistant panel이 아니며, 사용자가 부르거나
Manager가 intervention policy를 통과한 상황을 보고할 때 가능한 한 같은 음성
conversation을 이어간다.

네 가지 사용 시나리오:

1. Invocation — Floe를 부른다.
2. Capture — 생각이나 요청을 빠르게 말한다.
3. Transcription — 회의/대화를 기록하고 구조화한다.
4. Report — Floe가 허가된 맥락에서 중요한 변화를 적절한 순간에 먼저 알린다.

```text
user invocation or policy-approved report
→ streaming voice session
→ one Manager and the same AgentSession semantics
→ optional Expert delegation
→ spoken answer or visual approval/report handoff
```

Voice만으로 안전하고 명확하게 처리할 수 없는 mutation, 민감한 consent, 복잡한 선택,
provenance 확인과 recovery에는 UI가 나타난다. Voice confirmation은 action policy가
허용하고 transaction-bound 질문에 사용자가 명확히 응답한 경우에만 approval로
인정한다.

## Slice 순서

- S4의 text AgentSession과 AgentCommand/Event contract가 먼저다.
- S4 panel은 semantic loop를 관찰하고 검증하는 임시 primary surface일 뿐, 장기 제품의
  기본 interaction hierarchy를 정의하지 않는다.
- S6는 press-to-talk, streaming transcript, TTS/barge-in과 명시적으로 시작하는
  transcription session을 같은 Agent/Review boundary로 연결한다.
- S7은 on-device wake detection과 resident Device Agent에서 S6 session을 연다.
- S9 proactive intervention은 wake-up과 별도 정책이며, wake phrase는 Agent에게
  먼저 말할 권한을 주지 않는다.

Voice transport가 별도 Manager, Memory, Expert registry나 action authority를
만들어서는 안 된다.

## Ambient does not mean surveillance

Background understanding은 허가된 OS lifecycle과 connector change stream이 bounded
Situation 후보를 만드는 것을 뜻한다. 항상 microphone, 화면, precise location이나 raw
activity를 수집한다는 뜻이 아니다. 각 source는 scope, freshness, retention과 revoke를
지키고, Manager는 candidate마다 즉시 말하기, 미루기, 조용한 notification 또는 침묵을
선택한다.

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
