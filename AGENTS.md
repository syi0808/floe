# 개발 가이드라인

이 프로젝트는 **Floe AI** 어시스턴트 코드베이스를 관리합니다. 기여자는 아래 규칙을 준수하여 작업하십시오.

## 환경

- **Python 3.10** 이상 사용  
- 의존성 목록: `requirements.txt`, `pyproject.toml`  
- 가상 환경 생성 후 `pip install -r requirements.txt` 실행

## 테스트

- 변경 사항을 커밋하기 전 **`pytest`** 실행하여 모든 테스트 통과 확인  
- `pytest.ini`에서 `deprecated/` 디렉터리는 무시됨  
- 새 테스트는 반드시 `tests/` 하위에 추가

## 문서화

- 기획 문서 위치: `docs/`  
  - 단기 목표: `./short-term-plan/yyyyMMdd_hhmmss.md` 형식 사용
  - 장기 목표: `./long-term-plan/yyyyMMdd_hhmmss.md` 형식 사용
  - 주요 개발 기록: `./records/yyyyMMdd_hhmmss.md` 형식 사용
 
### 목표 문서화 템플릿

```
---
# YAML Front‑Matter: Codex Friendly Metadata

id: codex\_task\_\<YYYYMMDD\_HHMM>
author: <작성자>
date: [YYYY-MM-DDThh\:mm+09:00](YYYY-MM-DDThh:mm+09:00)
model\_context: "python3.10"           # 실행 언어/환경
task\_type: \<feature|bug|refactor|test|doc>
priority: \<P0|P1|P2>
linked\_issues: \["<#123>"]
---------------------------

## 1. Problem Statement

<코드·시스템이 해결해야 하는 문제를 한두 문장으로 요약>

## 2. Input Context

* **파일/모듈**: `<path/to/file.py>`
* **데이터/API**: `<endpoint>`
* **예시 프롬프트** *(선택)*:

\`\`\`
# 사용자: ...
\`\`\`

## 3. Expected Output / Success Criteria

| # | 성공 조건 | 검증 방법                |
| - | ----- | -------------------- |
| 1 | <조건>  | \<unittest, mypy, 등> |

## 4. Constraints

* <성능, 메모리, 라이브러리 제한>
* <코딩 스타일: PEP8, type hints 필수 등>

## 5. Step‑by‑Step Outline (for AI)

1. <고수준 단계>
2. <...>

## 6. Code Skeleton *(필요 시)*

\`\`\`python
# 여기에 필요한 함수/클래스 시그니처 작성
\`\`\`

## 7. Tests / Assertions

\`\`\`python
# pytest 스타일 예시
\`\`\`

## 8. Post‑Generation Checks

* [ ] `pytest` 통과
* [ ] `ruff` / `black` 포맷팅 확인
* [ ] `mypy` 타입 검사 통과

## 9. References

* <관련 RFC/ADR/문서 링크>

## 10. Revision History

| 버전  | 날짜           | 변경 내역 |
| --- | ------------ | ----- |
| 0.1 | <YYYY-MM-DD> | 최초 작성 |
```
 
### 개발 기록 문서화 템플릿

```
---
# YAML 메타데이터 – 자동 파싱/검색용
id: # 파일명과 동일
author: <작성자>
date: [YYYY-MM-DDThh\:mm+09:00](YYYY-MM-DDThh:mm+09:00)
scope: <모듈/영역>
linked\_issues: \["<#123>"]
commit\_range: "<a1b2c3d>..<e4f5g6h>"
---

## 1. 목표(Goal)

* <여기에 정량·정성 목표를 간결히 기재>

## 2. 수행 작업(Key Work Items)

| 구분  | 설명   | PR/커밋     |
| --- | ---- | --------- |
| 기능  | <설명> | <#456>    |
| 리팩터 | <설명> | <a1b2c3d> |
| 테스트 | <설명> | <#457>    |

## 3. 세부 변경(Details)

* <변경 요약: 주요 파일·라인>

## 4. 테스트 결과(Test Evidence)

\`\`\`bash
<pytest -q 결과>
<부하 테스트 요약>
\`\`\`

## 5. 의사결정(Decisions)

| 번호             | 선택지         | 결정       | 근거      |
| -------------- | ----------- | -------- | ------- |
| D-<YYYYMMDD>-1 | <옵션 A vs B> | **<선택>** | <근거 요약> |

> 큰 결정은 `docs/adr/ADR-XXX-<slug>.md` 참조

## 6. 이슈·회고(Lessons & Issues)

* **#태그**: <이슈·교훈 요약>

## 7. 다음 단계(Next Actions)

* [ ] <작업 설명> (담당: <이름>, \<D+N 또는 날짜>)

## 8. 참고(References)

* <외부/내부 문서 링크>
```

## 커밋 가이드라인

- 변경 사항을 명확히 요약한 커밋 메시지 작성  
- 필요 시 관련 계획·문서 업데이트를 동일 커밋에 포함
