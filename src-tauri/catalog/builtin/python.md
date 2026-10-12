---
kind: rule
title: Python 규칙
title_en: Python rules
summary: Python 파일에 타입 힌트·ruff·pytest 규칙을 적용해요
summary_en: Python conventions for .py files
detail: .py 파일을 다룰 때만 적용돼요. 타입 힌트를 달고, ruff·pytest 같은 표준 도구를 쓰고, 예외는 구체적으로 잡게 해요.
detail_en: Applies only to .py files: type hints, standard tools (ruff, pytest), and catching specific exceptions.
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
