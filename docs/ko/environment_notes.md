# 환경 설정 노트

지역 테스트 중에 일부 환경 문제가 발생했습니다.

- 리포지토리는 `.python-version`을 통해 Python 3.13.0을 지정합니다. 이 버전이 없을 수 있으므로 `pyenv local 3.10.17` 등을 사용해 버전을 맞추어야 합니다.
- 버전을 변경한 뒤 `pip install -r requirements.txt`를 실행하여 `requests` 같은 패키지를 설치합니다.
- 일부 테스트는 `pytest-cov`에 의존하므로 커버리지가 필요하면 `pip install pytest-cov`로 설치합니다.
