# Environment Setup Notes

During local testing some environment issues were encountered:

- The repository specifies Python 3.13.0 via `.python-version`. This exact version may not be installed. Using `pyenv local 3.10.17` or another available version works for running tests.
- Required dependencies must be installed using `pip install -r requirements.txt` prior to running the test suite.
- Some tests rely on `pytest-cov`; install it with `pip install pytest-cov` if coverage reports are desired.

