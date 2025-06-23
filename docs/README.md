# Floe Documentation

This directory contains design documents, implementation plans, and other notes for the Floe project.

- Core design docs such as `implementation_plan.md` and `remaining_work_plan.md` live here.
- Older planning files and work summaries have been moved to the [`archive/`](archive/) folder.

Refer to `service_completion_tasks.md` for a high-level overview of outstanding work.

## Development Dependencies

The codebase relies on several external libraries, most notably:

- `pydantic` for data validation
- `langdetect` for language detection
- `litellm` as the LLM client
- Google API clients (`google-api-python-client`, `google-auth-httplib2`,
  `google-auth-oauthlib`)

These packages are enumerated in `requirements.txt` and `pyproject.toml` and
should be installed before running the application or tests.

