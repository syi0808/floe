# Refactoring evidence history

현재 실행 지침은 [활성 문서판](../README.md), 가변 진행 상태는 [migration-ledger.md](../migration-ledger.md)다. 이 디렉터리는 과거 근거를 보존한다.

- [migration-ledger-through-89452eb.md](migration-ledger-through-89452eb.md): 원본 전체 checkpoint·P/T 이력. Git blob `950cf658ffef69cbb0e0bb3ec77964967cde5bb6`을 내용 변경 없이 이동했다. 옛 상단 상태와 다음 작업은 현행 지시가 아니다.
- [R001 bundle](../versions/r001-initial/README.md): 최초 계획·그래프·도구 전체.
- [R002 snapshot](../versions/r002-sequential/README.md): 직전 계획·프롬프트와 당시 축약 원장.

원래 상대 링크가 필요하면 [89452eb의 ledger 원본](https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/docs/architecture/migration-ledger.md)에서 읽는다. 더 오래된 제품 진행표는 [같은 commit의 PROGRESS](https://github.com/syi0808/floe/blob/89452eb5523ef6b1c76b7fe857095748d76de22d/PROGRESS.md)와 [제품 이력](../../history/README.md)을 참고한다.

과거 통과는 해당 commit·환경·대역·실행 범위에만 유효하다. 새 문서 발행이나 파일 이동으로 acceptance가 상승하지 않는다.
