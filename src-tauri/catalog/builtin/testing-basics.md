---
kind: instruction
title: 테스트 습관
summary: 버그 수정엔 재현 테스트를, 새 기능엔 핵심 경로 테스트를 추가하게 해요.
category: 코드 품질
---
## Testing

- When fixing a bug, first add a test that reproduces it, then make it pass.
- New behavior gets tests for the main path and the important edge cases.
- Never delete, skip, or weaken a failing test to make the suite pass. Fix the cause or report it.
- Run only the relevant tests while iterating, then the full suite before finishing.
