# 플로어 문서

이 디렉토리에는 디자인 문서, 구현 계획 및 FLOE 프로젝트에 대한 기타 메모가 포함되어 있습니다.

-`utubleation_plan.md` 및`remaning_work_plan.md`와 같은 핵심 디자인 문서.
- 기존 계획 파일 및 작업 요약이 [`archive/`] (Archive/) 폴더로 이동되었습니다.

뛰어난 작업에 대한 높은 수준의 개요는`service_completion_tasks.md`를 참조하십시오.

## 개발 종속성

코드베이스는 여러 외부 라이브러리에 의존합니다.

- 데이터 유효성 검사를위한 'Pydantic'
- 언어 탐지를위한‘langdetect`
- LLM 클라이언트로서 'litellm'
-Google API 클라이언트 (`Google-Api-Python-Client ',`Google-Auth-HTTPlib2`,
`google-auth-oauthlib`)

이 패키지는`re impings.txt` 및`pyproject.toml`에 열거됩니다
응용 프로그램 또는 테스트를 실행하기 전에 설치해야합니다.

로컬 설정 Quirks에 대한 추가 메모는 [`Environment_Notes.md`] (Environment_Notes.md)를 참조하십시오.
