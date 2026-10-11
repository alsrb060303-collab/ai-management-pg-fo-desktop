---
kind: instruction
title: 보안 기본
title_en: Security basics
summary: 비밀 값을 코드·로그에 남기지 않고, 외부 입력을 검증하게 해요.
summary_en: Keeps secrets out of code and logs, and validates outside input.
category: 보안
category_en: Security
---
## Security

- Never hard-code secrets, tokens, or passwords. Read them from the environment or the project's secret store.
- Do not print secrets in logs, errors, or test output.
- Validate and escape external input (user input, files, network data) before using it in queries, shell commands, or HTML.
- Flag any change that weakens authentication, authorization, or input validation.
