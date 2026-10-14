---
kind: instruction
title: 테스트 습관
title_en: Testing habits
summary: 버그 고치면 재현 테스트, 기능엔 테스트를 붙여요
summary_en: Adds tests with fixes and features
detail: 버그를 고칠 때는 그 버그를 재현하는 테스트를 먼저 쓰고, 새 기능엔 핵심 동작을 확인하는 테스트를 추가해요. 테스트를 지우거나 건너뛰어서 통과시키지 않아요.
detail_en: Writes a test that reproduces a bug before fixing it, and adds tests for the main path of new features. Never deletes or skips tests to get green.
category: 코드 품질
category_en: Code quality
tokens: 1
performance: 1
quality: 1
---
## Testing

- When fixing a bug, first add a test that reproduces it, then make it pass.
- New behavior gets tests for the main path and the important edge cases.
- Never delete, skip, or weaken a failing test to make the suite pass. Fix the cause or report it.
- Run only the relevant tests while iterating, then the full suite before finishing.
