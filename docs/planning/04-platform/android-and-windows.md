# Android & Windows

> Status: First-class platforms

## 이유

Floe는 Apple-only 비서가 아니다.

본인뿐 아니라 부모님 등 실제 가족에게 설치할 수 있는 신뢰할 만한 비서를 목표로 하므로 Android/Windows는 처음부터 abstraction 설계에 포함한다.

## Android

역할:

- mobile Day Canvas
- 빠른 capture
- assistant-level integration
- Quick Settings / shortcut
- notifications
- Health Connect
- local model 실행
- device/context source

## Health

```text
Samsung Health / Fitbit / etc.
        ↓
Health Connect
        ↓
Floe Local Health Engine
        ↓
Derived Health State
```

를 기본 conceptual architecture로 본다.

## Windows

macOS와 동급의 desktop surface를 목표로 한다.

- ambient wake
- global hotkey
- desktop context
- meeting transcription
- full Day Canvas
- local model runtime
- background helper/device agent

## Experience Parity

Android/iOS, Windows/macOS가 내부적으로 다른 API를 쓰더라도 사용자에게는 모두:

- 빠르게 부를 수 있고
- 하루를 볼 수 있고
- 기억이 이어지고
- 연결된 데이터를 활용하는

동일한 Floe 경험을 제공해야 한다.
