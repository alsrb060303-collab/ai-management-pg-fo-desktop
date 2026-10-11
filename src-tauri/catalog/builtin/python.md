---
kind: rule
title: Python 규칙
title_en: Python rules
summary: 타입 힌트, 표준 도구(ruff·pytest), 명확한 예외 처리. .py 파일에만 적용돼요.
summary_en: Type hints, standard tools (ruff, pytest), clear exception handling. Applies to .py files only.
category: Python
category_en: Python
paths:
  - "**/*.py"
---
# Python

- Add type hints to public functions and keep the type checker passing.
- Format and lint with the project's tools (for example ruff); do not mix formatters.
- Catch specific exceptions; never use a bare `except:`.
- Write tests with pytest; use fixtures instead of global state.
- Use `pathlib` for paths and context managers for files and resources.
