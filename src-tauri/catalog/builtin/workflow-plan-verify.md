---
kind: instruction
title: 계획 → 구현 → 검증
title_en: Plan, build, verify
summary: 코드 보고 → 계획 → 고치고 → 직접 확인해요
summary_en: Reads, plans, changes, then checks its work
detail: 바로 고치지 않고 관련 파일을 먼저 읽은 뒤 짧은 계획을 말해요. 필요한 만큼만 고치고, 끝나면 빌드·린트·테스트를 직접 돌려서 통과를 확인한 다음에 완료라고 해요. 확인을 못 했으면 못 했다고 말해요.
detail_en: Reads the relevant files and states a short plan before changing anything, makes the smallest fix, then runs build, lint and tests itself before calling it done. If it couldn't run a check, it says so.
category: 작업 방식
category_en: Workflow
tokens: 3
performance: 1
quality: 1
---
## Workflow

- Before changing code, read the relevant files and state a short plan for anything bigger than a one-line fix.
- Make the smallest change that solves the task. Do not refactor unrelated code.
- After changing code, run the project's checks (build, lint, tests) and fix what fails before reporting done.
- If a check cannot be run, say so explicitly instead of assuming it passes.
