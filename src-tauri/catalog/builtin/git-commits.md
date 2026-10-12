---
kind: instruction
title: Git 커밋 규칙
title_en: Git commit rules
summary: 커밋을 작게 나누고 히스토리는 건드리지 않아요
summary_en: Small commits, no history rewriting
detail: 한 커밋에 한 가지 목적만 담고, 이유가 드러나는 메시지를 써요. force push·rebase처럼 히스토리를 바꾸는 명령은 사용자가 요청할 때만 해요. 나중에 변경을 추적하거나 되돌리기 쉬워져요.
detail_en: One purpose per commit, with a message that says why. Force pushes, rebases and other history rewrites only when you ask. Makes changes easy to trace and revert later.
category: Git
category_en: Git
---
## Git

- Make small, focused commits. Each commit should build and pass checks.
- Commit message: imperative subject line under 72 characters, then a blank line and the reason for the change.
- Never force-push, rebase shared branches, or amend pushed commits unless asked.
- Do not commit generated files, secrets, or local config.
