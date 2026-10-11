---
kind: instruction
title: 계획 → 구현 → 검증
title_en: Plan, build, verify
summary: 바로 고치지 말고 먼저 살펴보고, 끝나면 직접 확인하게 해요.
summary_en: Look before changing anything, and check the result yourself when done.
category: 작업 방식
category_en: Workflow
---
## Workflow

- Before changing code, read the relevant files and state a short plan for anything bigger than a one-line fix.
- Make the smallest change that solves the task. Do not refactor unrelated code.
- After changing code, run the project's checks (build, lint, tests) and fix what fails before reporting done.
- If a check cannot be run, say so explicitly instead of assuming it passes.
