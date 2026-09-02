# Local AI Runtime Implementation

> Status: Technical direction, model choices remain domain-owned

## Principle

Floe는 "하나의 local LLM runtime"을 제품 전체에 강요하지 않는다.

도메인 로직이 요구하는 모델 특성에 따라 runtime을 선택한다.

```text
Domain Service
    ↓
Required Model Artifact
    ↓
Compatible Local Backend
```

## Workload Types

### Heuristic / Statistical

예:

- health baseline
- threshold
- time conflict
- deterministic privacy rule

LLM을 사용하지 않는다.

### Tiny Classifier / Encoder

예:

- PII detection
- intent classification
- intervention relevance
- entity classification

ONNX/Core ML 등 작은 runtime이 유리할 수 있다.

### Small Generative Model

예:

- local extraction
- sensitive summarization
- redaction
- short semantic interpretation

llama.cpp 계열이나 platform-optimized runtime을 고려한다.

### Strong Reasoning

정제된 context를:

- subscription provider
- remote API
- powerful self-host model

에 전달할 수 있다.

---

# Platform Optimization

## Apple

후보:

- Core ML
- Apple-provided on-device model APIs
- MLX 계열 runtime where deployable/appropriate
- llama.cpp Metal

실제 model별 benchmark로 결정한다.

## Android

후보:

- ONNX Runtime / platform NN acceleration
- llama.cpp Vulkan/CPU
- vendor acceleration where maintainable

## Windows

후보:

- ONNX Runtime / DirectML
- llama.cpp Vulkan/CUDA where available

## Desktop High-end

사용자가 강한 GPU/Apple Silicon을 가지고 있다면 larger local model을 optional inference resource로 등록할 수 있다.

---

# No Central Smart Router

다음은 피한다.

```text
InferenceRequest
    ↓
global router guesses:
  privacy?
  model size?
  cost?
  semantics?
```

대신 Health, Memory, Manager 등 각 domain component가 어떤 모델 artifact/runtime class를 요구하는지 명시한다.

공통 infrastructure는:

- model loading
- availability
- retry/fallback
- lifecycle
- resource accounting

정도만 제공한다.

---

# Model Package

장기적으로 재사용 가능한 모델 artifact를 다음과 같이 관리할 수 있다.

```text
models/
├─ health-state/
├─ memory-extractor/
├─ pii-redactor/
├─ intervention-ranker/
└─ generic-local-language/
```

package metadata 후보:

- model version
- input/output schema
- supported runtime/backend
- minimum memory
- quantization
- privacy class
- benchmark profile

"privacy class"는 자동 routing rule이 아니라 해당 artifact의 사용 가능 범위를 문서화하는 metadata다.
