# Performance Design

> Status: Engineering principles

## Goal

"크로스플랫폼이므로 느려도 된다"를 허용하지 않는다.

동시에 실제 병목이 아닌 control-plane을 미리 micro-optimize하지 않는다.

---

# UI

목표:

- 60Hz는 기본
- 지원 device에서는 120Hz interaction을 염두
- scrolling / Day Canvas manipulation에서 frame drop 최소화

원칙:

- expensive domain processing을 Dart UI isolate에서 수행하지 않음
- 대형 목록은 virtualization/lazy rendering
- immutable/batched snapshot
- FFI를 build/layout/frame마다 호출하지 않음
- animation 중 blocking I/O 금지

---

# Cross-language Boundary

## Good

```text
Rust state changes
→ one batched snapshot
→ Dart
→ many widgets render
```

## Bad

```text
100 widgets
→ 100 FFI calls
```

Serialization 자체보다 호출 횟수와 ownership copy가 더 큰 문제가 될 수 있다.

---

# Audio / Wake Word

가장 중요한 hot path 중 하나.

```text
Microphone callback
→ ring buffer
→ VAD
→ wake model
→ STT
```

이 pipeline은 Dart UI를 통과하지 않는다.

UI에는:

- listening
- activated
- partial transcript
- final transcript

와 같은 coarse event만 전달한다.

---

# Health

수천 개 sample을 Dart/remote LLM에 반복 전달하지 않는다.

```text
raw samples
→ local batch processing
→ derived features/state
```

---

# Database

- N+1 query를 domain API에서 금지
- snapshot query profile 유지
- sync/memory index에 explicit index
- vector search가 필요한 경우 실제 corpus 규모로 benchmark 후 기술 선택

---

# Network

Floe interaction 대부분은 AI/model latency가 dominant할 가능성이 높다.

control-plane JSON을 binary로 바꾸는 것보다:

- unnecessary round-trip 제거
- local-first action
- prefetch
- streaming
- optimistic UI

를 우선한다.

---

# Measurement

성능 최적화는 platform별 trace를 기준으로 한다.

최소 benchmark suite 후보:

```text
Day Canvas 10 / 100 / 1000 items
Memory retrieval corpus sizes
Sync batch 100 / 1k / 10k changes
Voice partial transcript latency
Wake idle CPU
Local model cold/warm latency
Flutter ↔ Rust batch FFI
Connector bootstrap throughput
```

## Performance Budget

정확한 숫자는 PoC 이후 고정한다.

초기에는 metric을 먼저 측정하고, release gate로 사용할 수 있는 budget을 도입한다.

---

# Priority

성능 최적화 우선순위:

1. user-perceived latency
2. battery/idle CPU
3. frame stability
4. memory footprint
5. cold start
6. throughput
7. binary size

Floe는 항상 주변에 있는 비서이므로 peak benchmark보다 **idle efficiency와 tail latency**가 특히 중요하다.
