# Floe 개선 구현 계획 — 시작점

**먼저 `IMPLEMENTATION_PLAN.html` 또는 `IMPLEMENTATION_PLAN.md`를 연다.** 동일한 내용이며 이전 대화가 필요 없다. 이 계획은 GitHub 소스 변경이나 제품 테스트 완료물이 아니다.

## 내용

- 본문: 승인 구조·계약·원자성·상태 소유권·코드 라인 window·구현 순서·전체 회귀 조건.
- `work-packages/P00.md`부터 `P25.md`: 담당 작업 배정용 사본. 공통 N01–N12/I01–I16은 본문을 참조한다.
- `data/`: 승인 module DAG, 원본 anchor JSON/CSV, 작업·회귀 추적 JSON/CSV, 실제 문서 검증 결과.
- `scripts/`: 읽기 전용 baseline/DAG/계획 검사와 도구 자체 테스트.
- `graphs/`: 승인된 v0.2 목표 그래프. 현재 소스의 자동 추출 그래프가 아니다.

## 실행

Python 3.11 이상이 필요하다. 아래 도구는 파일·Git 상태를 바꾸지 않는다.

```sh
python3 scripts/check_architecture.py --policy-only
python3 scripts/validate_plan.py
python3 -m unittest discover -s scripts -p 'test_*.py' -v
python3 scripts/verify_baseline.py /path/to/floe --json-out /tmp/floe-baseline.json
python3 scripts/check_architecture.py /path/to/floe --mode migration
# 최종 전환 뒤:
python3 scripts/check_architecture.py /path/to/floe --mode final
```

`RANGE_REBASE_REQUIRED`는 원본 심볼에 맞춰 window를 재확인하라는 뜻이다. 숫자에 맞추어 소스를 수정하지 않는다. migration mode는 동명 package의 승인된 구형 경로를 한시 허용하지만 target→legacy 의존은 거절한다. final mode는 22개 목표 경로와 허용 간선만 통과시킨다.

도구 자체의 14개 테스트를 Floe 애플리케이션 테스트 통과라고 해석하지 않는다. 실제 코드/build/native/LLM의 상태는 모두 별도로 검증한다.

## 진행 관리

본문이 명세의 기준이며 패키지별 문서는 배정용 사본이다. 실제 진행과 target commit/line range는 **저장소의 한 `docs/architecture/migration-ledger.md`**에 기록한다. 본문·패키지 사본·PROGRESS에 각각 서로 다른 상태를 쓰지 않는다. 코드 없는 빈 facade, 새 이름으로 감싼 구형 root, 무기한 compatibility bridge는 완료가 아니다.
