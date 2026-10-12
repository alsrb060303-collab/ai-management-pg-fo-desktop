---
kind: instruction
title: 보안 기본
title_en: Security basics
summary: 비밀 값을 숨기고 외부 입력을 검사해요
summary_en: Keeps secrets out, checks outside input
detail: API 키·비밀번호를 코드·로그·커밋에 남기지 않고 환경 변수나 설정으로 빼요. 사용자 입력·외부 데이터는 쓰기 전에 검증하고, SQL은 파라미터로 넘겨요. 실수로 비밀이 새는 일을 막아줘요.
detail_en: Keeps API keys and passwords out of code, logs and commits (env vars or config instead), validates user and external input before use, and passes SQL parameters safely. Guards against accidental leaks.
category: 보안
category_en: Security
---
## Security

- Never hard-code secrets, tokens, or passwords. Read them from the environment or the project's secret store.
- Do not print secrets in logs, errors, or test output.
- Validate and escape external input (user input, files, network data) before using it in queries, shell commands, or HTML.
- Flag any change that weakens authentication, authorization, or input validation.
