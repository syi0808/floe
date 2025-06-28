# Repository Development Guidelines

This project houses the Floe AI assistant codebase. Contributors should follow these basic rules when working in this repository.

## Environment

- Use **Python 3.10** or later. The dependency list is in `requirements.txt` and `pyproject.toml`.
- Create a virtual environment and install dependencies with `pip install -r requirements.txt`.

## Testing

- Run `pytest` before committing any changes to ensure all tests pass.
- The configuration in `pytest.ini` ignores the `deprecated` directory. Make sure new tests are placed under `tests/`.

## Documentation

- Planning documents live under `docs/`. Refer to `docs/remaining_work_plan.md` and `work_plan.md` for current tasks and long‑term goals.
- Add new plan files using the format `docs/plan_YYYYMMDD_HHMM.md` to log major development sessions.

## Commit Guidelines

- Provide clear commit messages summarizing the change.
- Include relevant plan and documentation updates in the same commit when appropriate.


